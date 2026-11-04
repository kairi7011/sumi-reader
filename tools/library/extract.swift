import Foundation
import PDFKit

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(1)
}

func emit(_ value: [String: Any]) {
    do {
        let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data([10]))
    } catch { fail("Could not encode extracted PDF data.") }
}

guard CommandLine.arguments.count == 2 else { fail("Usage: pdf-extract FILE.pdf") }
let fileURL = URL(fileURLWithPath: CommandLine.arguments[1])
guard let document = PDFDocument(url: fileURL) else { fail("PDFKit could not open the PDF.") }
guard !document.isLocked else { fail("Locked PDFs are not indexed. Open an unlocked copy explicitly.") }
guard document.allowsCopying else { fail("This PDF does not permit text copying.") }
guard document.pageCount > 0 && document.pageCount <= 20_000 else { fail("PDF page count is outside the supported range.") }

var outline: [[String: Any]] = []
func visit(_ node: PDFOutline, depth: Int) {
    guard depth < 16, outline.count < 2_000 else { return }
    for index in 0..<node.numberOfChildren {
        guard let child = node.child(at: index) else { continue }
        var entry: [String: Any] = ["title": String((child.label ?? "").prefix(500)), "depth": depth]
        let target = child.destination ?? (child.action as? PDFActionGoTo)?.destination
        if let page = target?.page {
            let pageIndex = document.index(for: page)
            if pageIndex != NSNotFound { entry["page"] = pageIndex + 1 }
        }
        outline.append(entry)
        visit(child, depth: depth + 1)
        if outline.count >= 2_000 { break }
    }
}
if let root = document.outlineRoot { visit(root, depth: 0) }
let attributes = document.documentAttributes ?? [:]
let rawTitle = (attributes[PDFDocumentAttribute.titleAttribute] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
let title = rawTitle.flatMap { $0.isEmpty ? nil : $0 } ?? fileURL.deletingPathExtension().lastPathComponent
emit(["type": "metadata", "title": String(title.prefix(500)), "pageCount": document.pageCount, "outline": outline])
for index in 0..<document.pageCount {
    autoreleasepool {
        guard let page = document.page(at: index) else { fail("Could not load PDF page \(index + 1).") }
        let text = page.string ?? ""
        guard text.count <= 200_000 else { fail("Page \(index + 1) exceeds the text limit.") }
        emit(["type": "page", "page": index + 1, "label": page.label ?? String(index + 1), "text": text])
    }
}
