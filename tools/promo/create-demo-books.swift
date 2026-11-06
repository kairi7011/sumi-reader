import AppKit
import PDFKit

// These notes are original demonstration material, independent of any real book.
struct Section {
    let heading: String
    let text: String
}
struct Chapter {
    let title: String
    let lead: String
    let sections: [Section]
    let note: String
}
struct Book {
    let filename: String
    let title: String
    let subtitle: String
    let color: NSColor
    let chapters: [Chapter]
}

func color(_ hex: UInt32) -> NSColor {
    NSColor(srgbRed: CGFloat((hex >> 16) & 255) / 255,
            green: CGFloat((hex >> 8) & 255) / 255,
            blue: CGFloat(hex & 255) / 255, alpha: 1)
}

let books: [Book] = [
    Book(filename: "interaction-design.pdf", title: "操作の設計",
         subtitle: "小さな応答から、迷わず使える画面へ。", color: color(0x43635A), chapters: [
        Chapter(title: "押した、その先を伝える",
                lead: "操作は、押した瞬間に終わりません。操作の結果を伝えるところまでを、一つの体験として考えます。",
                sections: [
                    Section(heading: "受け付けたことを、まず返す", text: "ボタンを押しても画面が変わらなければ、入力が届いたか分かりません。処理が終わる前でも、押された形や短い表示で応答できます。フィードバックは、待ち時間を飾るためではなく、今の状態を伝えるために使います。"),
                    Section(heading: "見た目と結果をそろえる", text: "押している間の色と、選択が続いている色には別の役割があります。一度押す操作なのか、状態を切り替える操作なのか。形、ラベル、変化の残り方をそろえると、次の操作を判断しやすくなります。")
                ], note: "確認すること\n押した直後と、処理が終わった後を見分けられるか。"),
        Chapter(title: "待ち時間の手がかり",
                lead: "待っている人に必要なのは、動きの多さよりも、処理が続いていると分かる手がかりです。",
                sections: [
                    Section(heading: "分かっている範囲を表示する", text: "何冊目を読み込んでいるかが分かるなら、その数を表示します。残り時間を正確に見積もれない処理には、無理に秒数を付けません。フィードバックの内容を実際の処理に合わせると、表示を信頼しやすくなります。"),
                    Section(heading: "やめる操作も用意する", text: "検索語を変えたら、古い検索の結果が後から割り込まないようにします。長い処理では、キャンセルした後に何が残るかも決めておきます。途中でやめられることと、やめたことが伝わることは、両方必要です。")
                ], note: "画面の例\n「3冊中2冊を検索中」と「検索を中止しました」を使い分ける。"),
        Chapter(title: "失敗しても戻れる",
                lead: "うまくいかなかった理由と、次にできることを近くに置きます。読み終えるだけのエラー表示にしません。",
                sections: [
                    Section(heading: "入力した内容を残す", text: "ファイルが見つからないときは、登録済みの書名や読書位置まで消す必要はありません。移動先を選び直せば続けられる形にします。再入力が必要な範囲を小さくすると、失敗から戻る手間を減らせます。"),
                    Section(heading: "結果を確認してから閉じる", text: "保存が完了したら、保存先と完了の表示を確かめられるようにします。失敗時のフィードバックには、理由だけでなく、選び直す、再試行する、戻るといった具体的な操作を添えます。")
                ], note: "確認すること\n失敗した直後に、利用者が次の一手を選べるか。"),
        Chapter(title: "手触りを確かめる",
                lead: "状態を一つずつ眺めるだけでなく、途中で操作を変えながら確かめると、小さな引っかかりが見えてきます。",
                sections: [
                    Section(heading: "押して、外へ動かし、離す", text: "ボタンを押したまま外へ移動し、そのまま離してみます。処理が始まる場所と始まらない場所は自然でしょうか。素早く続けて押した場合や、キーボードで操作した場合も、結果が食い違わないか確認します。"),
                    Section(heading: "読む時間を邪魔しない", text: "成功を伝える表示は、役目を終えたら静かに引きます。フィードバックがいつも目立ち続けると、本文や作業から注意がそれます。必要な瞬間に気づけて、その後は読み続けられる強さを探します。")
                ], note: "見直す順番\n入力の受付、処理中、完了、取り消しを通して確かめる。")
    ]),
    Book(filename: "search-design.pdf", title: "検索の設計",
         subtitle: "思い出した言葉から、必要な一節へ。", color: color(0x936A56), chapters: [
        Chapter(title: "言葉から本を探す",
                lead: "覚えているのは書名ではなく、一つの言葉かもしれません。探し始めるときの手がかりを広く受け止めます。",
                sections: [
                    Section(heading: "書名と本文を一緒に探す", text: "「フィードバック」という語を覚えていても、どの本で読んだかまでは思い出せないことがあります。本棚を横断して本文を探せると、本を一冊ずつ開く手間を省けます。書名が一致する候補と本文の候補は、区別して並べます。"),
                    Section(heading: "見つかった場所を添える", text: "検索結果には、書名、ページ番号、その前後の文章を表示します。語が含まれることだけでは、読みたい内容か判断できません。一節の文脈が見えると、開く前に候補を絞り込めます。")
                ], note: "試す検索語\nフィードバック / 読書位置 / ページ番号"),
        Chapter(title: "結果の前後を見せる",
                lead: "検索結果は本文への入口です。短く切り出しても、その文章が何について述べているかは残します。",
                sections: [
                    Section(heading: "語だけでなく、文脈を読む", text: "「フィードバックが必要」と「フィードバックが目立ちすぎる」では、同じ語でも話題が違います。検索語の前後を短い抜粋として示すと、この違いを判断できます。見出しも添えられるなら、章の中での位置が分かります。"),
                    Section(heading: "開いた後の場所をそろえる", text: "結果を選んだら、そのページを開き、該当箇所を示します。抜粋と本文が同じ場所を指していれば、探し直す必要がありません。検索の一覧に戻るときも、入力した語や選んでいた候補を残します。")
                ], note: "確認すること\n検索結果の抜粋から、本文の同じ一節へ進めるか。"),
        Chapter(title: "途中の結果も使える",
                lead: "複数の本を調べるときは、すべての処理が終わるまで一覧を空白にしておく必要はありません。",
                sections: [
                    Section(heading: "見つかった分から返す", text: "操作の結果を伝えるため、見つかった候補から表示し、残りの本の検索を続けます。「検索中」というフィードバックを添えれば、今の一覧が確定していないことも伝わります。結果が増えるたびに選択位置が飛ばないようにします。"),
                    Section(heading: "入力し直した意図を優先する", text: "短い語を入力した後、すぐに長い語へ直すことがあります。そのときは新しい検索を優先し、古い結果を混ぜません。処理を中止できることは、速度だけでなく、入力と表示の意味をそろえるためにも役立ちます。")
                ], note: "画面の例\n候補は順次表示し、検索が完了したら件数を確定する。"),
        Chapter(title: "見つからないとき",
                lead: "結果がないことと、検索できなかったことは別です。空の一覧だけで両方を伝えようとしません。",
                sections: [
                    Section(heading: "表記の違いを受け止める", text: "全角と半角、英字の大文字と小文字、PDFの改行などは、見た目が近くても検索に影響します。「フィードバック」が行をまたいでいても見つけられるようにすると、書かれ方の違いによる取りこぼしを減らせます。"),
                    Section(heading: "調べられた範囲を伝える", text: "画像だけのページや、文章の取り出しが許可されていない本は、本文を検索できない場合があります。その範囲を示すと、「結果なし」の意味が分かります。検索語を短くする方法や、目次から探す方法も残しておきます。")
                ], note: "確認すること\n「一致なし」と「この本は検索できない」を区別しているか。")
    ]),
    Book(filename: "reading-design.pdf", title: "読書の設計",
         subtitle: "読む場所を保ち、考えを持ち帰る。", color: color(0x566A7D), chapters: [
        Chapter(title: "本文のために余白を取る",
                lead: "読書中に長く見るのは本文です。操作の場所を分かりやすくしながら、画面の中心を文章に譲ります。",
                sections: [
                    Section(heading: "道具は必要なときに近くへ", text: "よく使うページ移動は小さく残し、設定は一か所にまとめます。本文のそばに多くのボタンを置けば便利になるとは限りません。読んでいる途中でも場所を思い出せる、少数の入口を用意します。"),
                    Section(heading: "静かでも応答は伝える", text: "落ち着いた画面でも、操作の結果を伝える手がかりは必要です。ページが変わった、しおりが付いた、文章をコピーした。そのフィードバックが短く伝われば、繰り返し操作して確かめる必要がなくなります。")
                ], note: "確認すること\n道具を探さずに使えて、その後すぐ本文へ戻れるか。"),
        Chapter(title: "読んでいた場所に戻る",
                lead: "一冊を順番に読み進めるだけでなく、別の章を見たり、別の本と比べたりする時間も読書に含めます。",
                sections: [
                    Section(heading: "ページの姿を手がかりにする", text: "図があるページや、章の始まりの大きな見出しは、記憶に残る手がかりです。ページ一覧で全体を見渡せると、番号を覚えていなくても探せます。数字で移動する入口と、見た目で選ぶ入口を両方用意します。"),
                    Section(heading: "本を切り替えても位置を保つ", text: "検索結果から別の本を開いた後も、元の本の読書位置を残します。切り替えたことへのフィードバックは、書名とページ番号で十分な場合もあります。読みかけの場所が保たれると、比較しながら読み進めやすくなります。")
                ], note: "読む場面\n本文を読む、本を比べる、元のページへ戻る。"),
        Chapter(title: "一節を持ち帰る",
                lead: "気になった文章を残すときは、内容と出どころを一緒に扱うと、後から読み直しやすくなります。",
                sections: [
                    Section(heading: "文章には場所を添える", text: "選択した文章と一緒に、書名とページ番号を記録します。後でノートを読み返したとき、前後の説明に戻れるからです。フィードバックを見直すメモであっても、一節だけで結論を決めず、元の文脈を確かめます。"),
                    Section(heading: "図と文章で残し方を選ぶ", text: "文章は選択してコピーすると、検索や編集に使いやすくなります。図や組版が意味を持つ部分は、範囲を絞った画像が役立ちます。目的に合わせて残し方を選び、不要なページ全体を抱え込まないようにします。")
                ], note: "残す内容\n気になった一節、書名、ページ番号、自分の問い。"),
        Chapter(title: "読むリズムを途切れさせない",
                lead: "ページを開く、必要な部分を確かめる、読み進める。短い操作の間にも、読んでいる場所が分かるようにします。",
                sections: [
                    Section(heading: "拡大しても戻り道を残す", text: "小さな文字や図を確かめたいときは、その場で拡大します。見終わった後に全体表示へ戻れる操作を分かりやすくしておけば、細部を見ることが読書の中断になりにくくなります。"),
                    Section(heading: "変化の強さを選ぶ", text: "動きのあるフィードバックは、ページが切り替わる方向を伝えられます。ただし、毎回の操作に長い演出は必要ありません。動きを減らしたい人の設定も尊重し、どちらの場合でも同じページへ確実に進めるようにします。")
                ], note: "確認すること\n操作の後で、視線と読書位置をすぐに取り戻せるか。")
    ])
]

let pageSize = CGSize(width: 500, height: 700)
let ink = color(0x252B29)
let muted = color(0x68716C)
let paper = color(0xFBFAF6)
let margin: CGFloat = 46

func font(_ size: CGFloat, bold: Bool = false) -> NSFont {
    NSFont(name: bold ? "HiraginoSans-W6" : "HiraginoSans-W3", size: size)
        ?? NSFont.systemFont(ofSize: size, weight: bold ? .semibold : .regular)
}

func fill(_ rect: CGRect, _ color: NSColor) {
    color.setFill()
    NSBezierPath(rect: CGRect(x: rect.minX, y: pageSize.height - rect.maxY, width: rect.width, height: rect.height)).fill()
}

@discardableResult
func text(_ value: String, x: CGFloat = margin, top: CGFloat, width: CGFloat = 408,
          size: CGFloat = 12.5, line: CGFloat = 23, bold: Bool = false,
          color: NSColor = ink, alignment: NSTextAlignment = .left) -> CGFloat {
    let paragraph = NSMutableParagraphStyle()
    paragraph.minimumLineHeight = line
    paragraph.maximumLineHeight = line
    paragraph.alignment = alignment
    paragraph.lineBreakMode = .byWordWrapping
    let attributes: [NSAttributedString.Key: Any] = [.font: font(size, bold: bold), .foregroundColor: color, .paragraphStyle: paragraph]
    let string = NSAttributedString(string: value, attributes: attributes)
    let options: NSString.DrawingOptions = [.usesLineFragmentOrigin, .usesFontLeading]
    let height = ceil(string.boundingRect(with: NSSize(width: width, height: 2000), options: options).height)
    precondition(top + height < pageSize.height - 8, "Text overflow: \(value.prefix(30))")
    string.draw(with: NSRect(x: x, y: pageSize.height - top - height, width: width, height: height), options: options)
    return height
}

func cover(_ book: Book, number: Int) {
    fill(CGRect(origin: .zero, size: pageSize), paper)
    fill(CGRect(x: 0, y: 0, width: 500, height: 494), book.color)
    let light = color(0xF6F4EB)
    text("読むための技術ノート", top: 45, size: 11, line: 18, color: light)
    text(String(format: "%02d", number), top: 90, size: 94, line: 110, bold: true,
         color: light.withAlphaComponent(0.24))
    fill(CGRect(x: margin, y: 238, width: 408, height: 1), light.withAlphaComponent(0.45))
    text(book.title, top: 268, size: 53, line: 70, bold: true, color: light)
    text(book.subtitle, top: 367, size: 14, line: 25, color: light)
    text("Sumi サンプルブック", top: 440, size: 10, line: 16, color: light.withAlphaComponent(0.8))
    for (index, chapter) in book.chapters.enumerated() {
        let y = CGFloat(523 + index * 29)
        text(String(format: "%02d", index + 1), top: y, width: 28, size: 10, line: 18, color: book.color)
        text(chapter.title, x: 84, top: y, width: 366, size: 12, line: 19)
    }
    text("公開デモ用に制作したオリジナルの文章です。", top: 662, size: 8.5, line: 14, color: muted)
}

func bodyPage(_ book: Book, chapter: Chapter, index: Int) {
    fill(CGRect(origin: .zero, size: pageSize), paper)
    fill(CGRect(x: margin, y: 37, width: 3, height: 13), book.color)
    text(book.title, x: 57, top: 34, width: 330, size: 9.5, line: 17, color: muted)
    text(String(format: "%02d", index + 1), x: 406, top: 34, width: 48, size: 10, line: 17, color: book.color, alignment: .right)
    text(chapter.title, top: 83, size: 24, line: 36, bold: true)
    fill(CGRect(x: margin, y: 135, width: 408, height: 0.8), book.color.withAlphaComponent(0.35))
    var y: CGFloat = 157
    y += text(chapter.lead, top: y, size: 13, line: 24, color: muted) + 22
    for section in chapter.sections {
        y += text(section.heading, top: y, size: 15, line: 23, bold: true) + 7
        y += text(section.text, top: y, size: 12, line: 21) + 20
    }
    precondition(y <= 564, "Chapter too long: \(book.title) / \(chapter.title), y=\(y)")
    fill(CGRect(x: margin, y: 570, width: 408, height: 68), book.color.withAlphaComponent(0.065))
    fill(CGRect(x: margin, y: 570, width: 2, height: 68), book.color.withAlphaComponent(0.6))
    text(chapter.note, x: 61, top: 580, width: 378, size: 10.5, line: 20, color: book.color)
    fill(CGRect(x: margin, y: 657, width: 408, height: 0.5), color(0xD9DDD6))
    text("読むための技術ノート", top: 668, size: 8, line: 13, color: muted)
    text("\(index + 2) / 5", x: 398, top: 668, width: 56, size: 8, line: 13, color: muted, alignment: .right)
}

let folder = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? "test-artifacts/promo-fixtures", isDirectory: true).standardizedFileURL
try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
var report: [[String: Any]] = []
for (number, book) in books.enumerated() {
    let destination = folder.appendingPathComponent(book.filename)
    var box = CGRect(origin: .zero, size: pageSize)
    guard let context = CGContext(destination as CFURL, mediaBox: &box, nil) else { fatalError("Cannot create PDF") }
    for index in 0...book.chapters.count {
        context.beginPDFPage(nil)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
        if index == 0 { cover(book, number: number + 1) }
        else { bodyPage(book, chapter: book.chapters[index - 1], index: index - 1) }
        NSGraphicsContext.restoreGraphicsState()
        context.endPDFPage()
    }
    context.closePDF()
    guard let document = PDFDocument(url: destination) else { fatalError("Cannot reopen PDF") }
    document.documentAttributes = [PDFDocumentAttribute.titleAttribute: book.title,
                                   PDFDocumentAttribute.authorAttribute: "Sumi",
                                   PDFDocumentAttribute.subjectAttribute: "公開デモ用のオリジナル技術ノート",
                                   PDFDocumentAttribute.creatorAttribute: "Sumi demo corpus generator"]
    let outline = PDFOutline()
    for (index, chapter) in book.chapters.enumerated() {
        let item = PDFOutline()
        item.label = chapter.title
        item.destination = PDFDestination(page: document.page(at: index + 1)!, at: NSPoint(x: 0, y: pageSize.height))
        outline.insertChild(item, at: index)
    }
    document.outlineRoot = outline
    precondition(document.write(to: destination), "Cannot save PDF metadata")
    let verified = PDFDocument(url: destination)!
    let pageTexts = (0..<verified.pageCount).map { verified.page(at: $0)?.string ?? "" }
    let searchableTexts = pageTexts.map { $0.replacingOccurrences(of: "\n", with: "") }
    let hits = searchableTexts.enumerated().compactMap { $0.element.contains("フィードバック") ? $0.offset + 1 : nil }
    let phrase = "操作の結果を伝える"
    let phraseCount = searchableTexts.reduce(0) { $0 + $1.components(separatedBy: phrase).count - 1 }
    precondition(verified.pageCount == 5 && hits.count >= 3, "Missing demo search text")
    precondition(pageTexts.allSatisfy { $0.count > 100 }, "Missing selectable text")
    precondition(phraseCount == 1, "The cross-book phrase must occur once per book")
    precondition(verified.documentAttributes?[PDFDocumentAttribute.titleAttribute] as? String == book.title, "Missing title")
    for pageIndex in 0..<verified.pageCount {
        let preview = verified.page(at: pageIndex)!.thumbnail(of: NSSize(width: 1000, height: 1400), for: .mediaBox)
        let bitmap = NSBitmapImageRep(data: preview.tiffRepresentation!)!
        let filename = destination.deletingPathExtension().lastPathComponent + "-page-\(pageIndex + 1).png"
        try bitmap.representation(using: .png, properties: [:])!.write(to: folder.appendingPathComponent(filename))
    }
    report.append(["file": book.filename, "title": book.title, "pages": verified.pageCount,
                   "charactersPerPage": pageTexts.map(\.count), "feedbackPages": hits,
                   "crossBookPhraseCount": phraseCount])
}
let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes])
try data.write(to: folder.appendingPathComponent("verification.json"))
print(String(data: data, encoding: .utf8)!)
