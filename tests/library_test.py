import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[1] / "tools/library/sumi_library.py"
SPEC = importlib.util.spec_from_file_location("sumi_library", SOURCE)
library = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(library)


class LocalLibraryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.folder = Path(self.temporary.name)
        self.connection = library.connect(self.folder, writable=True)
        library.create_schema(self.connection)
        self.path = self.folder / "oreilly-example.pdf"
        self.records = [
            {"type": "metadata", "title": "読書の設計", "pageCount": 3,
             "outline": [{"title": "フィードバック", "page": 2, "depth": 0}]},
            {"type": "page", "page": 1, "label": "i", "text": "目次と前書き。"},
            {"type": "page", "page": 2, "label": "1", "text": "操作のフィードバックを明確にする。認知負荷を減らす。ＡＢＣ\n良いデザイン"},
            {"type": "page", "page": 3, "label": "2", "text": "A reason to read. Local exact phrase search. A 100% useful tool."},
        ]
        self.book = library.store_book(self.connection, self.path, "digest", self.records)
        self.id = self.book["book_id"]

    def tearDown(self):
        self.connection.close()
        self.temporary.cleanup()

    def test_japanese_phrase_and_short_query_preserve_page_citations(self):
        for query in ("フィードバック", "認知", "認", "abc"):
            result = library.search_books(self.connection, query)
            self.assertEqual(len(result["matching_pages"]), 1)
            hit = result["matching_pages"][0]
            self.assertEqual((hit["pdf_page"], hit["page_label"]), (2, "1"))
            self.assertEqual(hit["title"], "読書の設計")
            self.assertNotIn("path", hit)

    def test_query_is_a_literal_phrase_not_sql_or_fts_syntax(self):
        self.assertEqual(library.search_books(self.connection, '" OR * --')["matching_pages"], [])
        self.assertEqual(library.search_books(self.connection, "100%")["matching_pages"][0]["pdf_page"], 3)
        self.assertEqual(library.search_books(self.connection, "read local")["matching_pages"], [])
        self.assertEqual(library.search_books(self.connection, "LOCAL EXACT PHRASE")["matching_pages"][0]["pdf_page"], 3)

    def test_normalization_joins_pdf_line_wrapping(self):
        self.assertEqual(library.normalize("フィード\nバック ＡＢＣ inter-\nface"), "フィードバック abc interface")

    def test_reindex_replaces_old_search_entries(self):
        updated = [dict(record) for record in self.records]
        updated[2]["text"] = "新しい章だけを検索する。"
        library.store_book(self.connection, self.path, "new", updated)
        self.assertEqual(library.search_books(self.connection, "フィードバック")["matching_pages"], [])
        self.assertEqual(len(library.search_books(self.connection, "新しい章")["matching_pages"]), 1)
        self.assertEqual(len(library.list_books(self.connection)["books"]), 1)

    def test_incomplete_extraction_keeps_previous_book(self):
        with self.assertRaises(library.LibraryError):
            library.store_book(self.connection, self.path, "failed", self.records[:-1])
        self.assertEqual(len(library.search_books(self.connection, "フィードバック")["matching_pages"]), 1)

    def test_page_reads_are_bounded_and_only_resolve_index_ids(self):
        result = library.read_book_pages(self.connection, self.id, 2, 2)
        self.assertEqual([page["pdf_page"] for page in result["pages"]], [2, 3])
        self.assertEqual(result["pages"][0]["page_label"], "1")
        for value in (str(self.path), "/etc/passwd", "../../secrets"):
            with self.assertRaises(library.LibraryError):
                library.read_book_pages(self.connection, value, 1)
        for count in (0, 6, True, "2"):
            with self.assertRaises(library.LibraryError):
                library.read_book_pages(self.connection, self.id, 1, count)
        with self.assertRaises(library.LibraryError):
            library.read_book_pages(self.connection, self.id, 4)

    def test_long_pages_report_truncation(self):
        records = [dict(record) for record in self.records]
        records[2]["text"] = "読書" * 4000
        library.store_book(self.connection, self.path, "long", records)
        page = library.read_book_pages(self.connection, self.id, 2)["pages"][0]
        self.assertEqual(len(page["text"]), 5000)
        self.assertTrue(page["truncated"])
        self.assertEqual(page["characters"], 8000)

    def test_book_filter_and_toc(self):
        self.assertEqual(library.search_books(self.connection, "読書", book_id="not-indexed")["matching_pages"], [])
        toc = library.table_of_contents(self.connection, self.id)
        self.assertEqual(toc["entries"][0]["page"], 2)

    def test_default_directory_scan_ignores_unrelated_pdfs(self):
        for filename in ("oreilly-example.pdf", "ticket.pdf", "notes.txt"):
            (self.folder / filename).write_text("fixture")
        found = library.source_files([str(self.folder)], "oreilly-*.pdf")
        self.assertEqual([path.name for path in found], ["oreilly-example.pdf"])

    def test_read_only_connection_rejects_mutations(self):
        connection = library.connect(self.folder)
        try:
            with self.assertRaises(library.sqlite3.OperationalError):
                connection.execute("DELETE FROM books")
        finally:
            connection.close()

    def request(self, method, params=None, identifier=1):
        return {"jsonrpc": "2.0", "id": identifier, "method": method, "params": params or {}}

    def legacy_server(self):
        server = library.MCPServer(self.folder)
        result = server.process(self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}))
        self.assertEqual(result["result"]["protocolVersion"], "2025-11-25")
        self.assertIsNone(server.process({"jsonrpc": "2.0", "method": "notifications/initialized"}))
        return server

    def test_legacy_mcp_handshake_and_read_only_tools(self):
        server = self.legacy_server()
        tools = server.process(self.request("tools/list"))["result"]["tools"]
        self.assertEqual(len(tools), 4)
        self.assertTrue(all(tool["annotations"]["readOnlyHint"] for tool in tools))
        result = server.process(self.request("tools/call", {"name": "search_books", "arguments": {"query": "認知"}}))["result"]
        self.assertFalse(result["isError"])
        self.assertEqual(result["structuredContent"]["matching_pages"][0]["pdf_page"], 2)

    def test_mcp_malformed_tool_name_and_path_do_not_crash_server(self):
        server = self.legacy_server()
        response = server.process(self.request("tools/call", {"name": ["read_book_pages"]}))
        self.assertEqual(response["error"]["code"], -32602)
        response = server.process(self.request("tools/call", {"name": "read_book_pages", "arguments": {"book_id": "/etc/passwd", "page": 1}}))
        self.assertTrue(response["result"]["isError"])
        response = server.process(self.request("tools/call", {"name": "read_book_pages", "arguments": {"book_id": self.id, "page": 1, "path": "/etc/passwd"}}))
        self.assertEqual(response["error"]["code"], -32602)

    def test_modern_mcp_discovery_and_per_request_metadata(self):
        server = library.MCPServer(self.folder)
        metadata = {"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {}}
        response = server.process(self.request("server/discover", {"_meta": metadata}))["result"]
        self.assertEqual(response["resultType"], "complete")
        self.assertEqual(response["supportedVersions"], ["2026-07-28"])
        response = server.process(self.request("tools/call", {"_meta": metadata, "name": "list_books"}))["result"]
        self.assertEqual(response["structuredContent"]["books"][0]["book_id"], self.id)
        self.assertIn("io.modelcontextprotocol/serverInfo", response["_meta"])
        self.assertIn("error", server.process(self.request("tools/list")))

    def test_stdio_keeps_one_json_response_per_line_and_honors_eof(self):
        messages = [b"{bad json}\n", json.dumps(self.request("ping")).encode() + b"\n"]
        output = io.StringIO()
        library.serve(self.folder, io.BytesIO(b"".join(messages)), output)
        replies = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertEqual(replies[0]["error"]["code"], -32700)
        self.assertEqual(replies[1]["result"], {})


if __name__ == "__main__":
    unittest.main()
