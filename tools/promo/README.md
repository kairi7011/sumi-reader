# 紹介用サンプルPDF

Sumiのスクリーンショットや動画に使う、オリジナルの技術ノート3冊を生成します。実在の書籍から文章、図版、表紙を転用していません。内容は操作例を示すために書いたもので、研究結果の紹介ではありません。

macOSに付属するAppKit、PDFKit、ヒラギノ角ゴシックを使います。外部パッケージや通信は不要です。Swiftの実行にはAppleのCommand Line Toolsが必要です。

リポジトリのルートで実行してください。

```sh
mkdir -p test-artifacts/promo-fixtures
/usr/bin/swiftc -O -framework AppKit -framework PDFKit tools/promo/create-demo-books.swift -o test-artifacts/promo-fixtures/create-demo-books
test-artifacts/promo-fixtures/create-demo-books
```

| ファイル | 書名 | ページ数 | 表紙 |
| --- | --- | --- | --- |
| `interaction-design.pdf` | 操作の設計 | 5 | 緑 |
| `search-design.pdf` | 検索の設計 | 5 | 赤茶 |
| `reading-design.pdf` | 読書の設計 | 5 | 青灰 |

出力先は`test-artifacts/promo-fixtures`です。生成したPDF、実行ファイル、検証結果はGitの追跡対象になりません。別の出力先は実行ファイルの第1引数に指定できます。

3冊をSumiへ追加し、本文検索で「操作の結果を伝える」を入力すると、各冊から1件ずつ見つかります。別々の書名を一つの検索結果に並べる場面に使えます。「フィードバック」は各冊の2〜5ページに含まれています。

各PDFには選択できる日本語の文章、書名メタデータ、章の目次を埋め込みます。生成後にPDFKitで開き直し、ページ数、書名、各ページの文字数、検索語を含むページを検証し、`verification.json`へ保存します。検索語の検証では組版による改行を除きます。確認用に全ページのPNG画像も同じフォルダへ出力します。

このディレクトリの文章と生成コードは、リポジトリと同じApache-2.0ライセンスで利用できます。公開する素材には、個人の本棚や読書履歴を混ぜず、このサンプルだけを使ってください。
