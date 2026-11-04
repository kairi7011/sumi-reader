import AppKit
import PDFKit

let folder = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
let output = folder.appendingPathComponent("日本語の読書テスト.pdf")
var box = CGRect(x: 0, y: 0, width: 595, height: 842)
let context = CGContext(output as CFURL, mediaBox: &box, nil)!
let regular = NSFont(name: "HiraginoSans-W3", size: 13)!
let title = NSFont(name: "HiraginoSans-W6", size: 25)!
for page in 1...3 {
    context.beginPDFPage(nil)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
    NSColor.white.setFill()
    NSBezierPath(rect: box).fill()
    ("読書と技術のノート" as NSString).draw(at: NSPoint(x: 55, y: 740), withAttributes: [.font: title, .foregroundColor: NSColor.black])
    let lines = [
        "第\(page)章　日本語の文章を読む",
        "読みやすさは、文字の大きさと余白から始まります。",
        "気になる一文を選んで、ページ番号と一緒にコピーします。",
        "図や表は、必要な範囲だけを画像として切り抜きます。",
        "検索語：日本語、技術書、クリップボード、PDF。",
        "0123456789 / API response / const value = 42;"
    ]
    for (index, line) in lines.enumerated() {
        (line as NSString).draw(at: NSPoint(x: 55, y: 680 - index * 42), withAttributes: [.font: regular, .foregroundColor: NSColor.black])
    }
    ("\(page)" as NSString).draw(at: NSPoint(x: 290, y: 45), withAttributes: [.font: regular])
    NSGraphicsContext.restoreGraphicsState()
    context.endPDFPage()
}
context.closePDF()
let document = PDFDocument(url: output)!
let protectedURL = folder.appendingPathComponent("パスワード付き.pdf")
document.write(to: protectedURL, withOptions: [.userPasswordOption: "sumi", .ownerPasswordOption: "sumi-test-owner"])
print(output.path)
print(protectedURL.path)
