# Sumiの操作を録画する

[English instructions](README.md)

このディレクトリには、紹介用のサンプルPDFを作るツールと、実際の操作を録画して整えるツールがあります。紹介動画では、文字を入力して検索し、結果を選んで本のページを開くところまでを続けて録画します。macOSのスクリーンショットツールバーか、後述するScreenCaptureKitの録画ツールを許可を得たうえで使ってください。ここにあるツールはアプリの画面操作を行いません。

ツールはAppleのAppKit、PDFKit、AVFoundationを使います。macOS 15以降とAppleのCommand Line Toolsが必要です。外部パッケージの追加やダウンロードは不要です。コマンドはリポジトリのルートで実行してください。

## 英語・日本語のサンプルを作る

```sh
swift tools/promo/create-demo-books.swift --language en --output test-artifacts/promo-fixtures-en
swift tools/promo/create-demo-books.swift --language ja --output test-artifacts/promo-fixtures-ja
```

どちらの言語でも、5ページのPDFを3冊作ります。本文は文字を選択でき、表紙、書名メタデータ、章の目次が入っています。生成後にPDFKitで開き直して本文と検索語を検証し、`verification.json`と全ページのPNG画像を保存します。文章、図版、表紙は実在の書籍から転用していません。

| ファイル | 英語の書名 | `feedback`が見つかるページ | 日本語の書名 |
| --- | --- | --- | --- |
| `interaction-design.pdf` | Interaction Design | 2 | 操作の設計 |
| `search-design.pdf` | Search Design | 4 | 検索の設計 |
| `reading-design.pdf` | Reading Design | 3 | 読書の設計 |

英語の`feedback`は、表にあるページに各冊1回ずつ出てきます。日本語では「操作の結果を伝える」を検索すると、各冊から1ページずつ見つかります。「フィードバック」では各冊の複数ページが見つかります。ページ数には表紙も含みます。

言語を省略すると日本語、出力先を省略すると`test-artifacts/promo-fixtures`になります。従来どおり、第1引数で出力先を指定する方法も使えます。`test-artifacts`内のPDF、画像、検証結果はGitの追跡対象になりません。文章と生成コードは、リポジトリと同じApache-2.0ライセンスで利用できます。本文は操作例を示すためのもので、研究結果の紹介ではありません。

## 実際の操作を録画する

紹介用には、この3冊だけを入れた本棚を使ってください。録画前にアプリの言語とサンプルの言語をそろえます。個人の書籍、ファイルパス、通知、ほかのウィンドウが録画範囲に入らないようにします。

1. サンプル3冊が見える本棚を開き、macOSのスクリーンショットツールバーからアプリの範囲を録画します。ナレーションを入れない場合はマイクをオフにしてください。
2. 検索を開き、英語では`feedback`、日本語では「操作の結果を伝える」を実際に入力します。結果がそろったら、3冊の書名を読めるよう少し待ちます。
3. 別の本の結果を選びます。英語では「Search Design」の4ページを選ぶと、移動先を確かめやすくなります。開いたページと一致する文章を見せてください。
4. ページめくり、単ページと見開きの切り替え、ページ一覧からの移動、拡大と全体表示への復帰、開いている本のタブ切り替えなどを紹介します。操作ごとに結果が見える時間を取ってください。
5. 録画を停止して、元のMOVをローカルに残します。映像を確認してから、残す時間と画面範囲を決めます。

検索からページを開くまでの操作は、途中で切らずに残してください。実際の速度と読み込みの様子もそのまま見せます。短くする場合は紹介する読書機能を減らし、待ち時間だけを切り落としたり、生成した静止画を実際の操作として見せたりしないでください。録画の開始と停止は、Appleの[画面収録の手順](https://support.apple.com/ja-jp/102618)で確認できます。

## 録画の前後と画面範囲を整える

```sh
swift tools/promo/process-recording.swift test-artifacts/recording.mov \
  --output test-artifacts/demo-en.mp4 \
  --start 2 --end 45 \
  --width 1920 --height 1080 \
  --mute \
  --previews test-artifacts/demo-en-preview \
  --report test-artifacts/demo-en.json
```

開始・終了時刻は、実際の録画に合わせて変えてください。省略すると動画全体を残します。出力形式はH.264のMP4です。画面収録でフレーム間隔が変わっていても、元の時間位置を保ちます。行う処理は、前後の切り取り、固定した画面範囲の切り出し、縦横比を保つ縮小だけです。UI、字幕、場面転換、静止フレームの追加や速度変更は行いません。

`--width`と`--height`は出力サイズの上限です。縦横比を変えたり、元より拡大したりはしません。H.264で扱えるよう、縦横のピクセル数は偶数に切り下げます。音声は`--mute`を指定した場合だけ取り除きます。同じ名前のMP4がある場合は上書きせずに終了します。

アプリの周囲を切り落とす場合は、`--crop X,Y,WIDTH,HEIGHT`を追加します。座標は、動画の向きを反映した表示の左上から測るピクセル数です。Retina環境では、ウィンドウのポイント座標と画像のピクセル座標が異なることがあります。実際の動画サイズを確認し、検索パネル、移動先のページ、ページ番号が収まる範囲を指定してください。切り出しが不要なら省略できます。

変換後は全フレームを読み取り、形式、サイズ、長さ、フレームの時刻順を検証してJSONに記録します。JSONには変換前後のファイルのSHA-256ハッシュも入ります。`--previews`を指定すると、冒頭、中央、末尾付近の画像も保存します。画像とMP4全体を見て、入力した文字、選んだ結果、開いたページが読めることを確かめてください。この検証は、映像の内容や個人情報の有無を判定するものではありません。

実装にはAppleの[画面範囲を指定するAPI](https://developer.apple.com/documentation/avfoundation/avmutablevideocompositionlayerinstruction/setcroprectangle%28_%3Aat%3A%29)と[動画を書き出すAPI](https://developer.apple.com/documentation/AVFoundation/AVAssetExportSession/export%28to%3Aas%3Aisolation%3A%29)を使っています。出力動画の付加メタデータを空にし、検証結果にはファイル名だけを記録します。ただし、ファイル名と映像に映った内容は公開前に確認してください。

## 許可されたウィンドウをScreenCaptureKitで録画する

`record-window.swift`は、明示的に選んだアプリのウィンドウと、その子ウィンドウを録画します。画面収録についてユーザーの許可を得てから実行してください。`--list`による一覧取得でも、macOSの権限画面が開く場合があります。`--allow-screen-capture`は実行者の意図を示す指定であり、システムの権限を回避したり、ユーザーの許可を証明したりするものではありません。

コンパイルだけなら撮影は始まりません。

```sh
swiftc -O tools/promo/record-window.swift -o test-artifacts/record-window
```

許可を得たあと、対象のbundle IDを指定して録画します。

```sh
test-artifacts/record-window --allow-screen-capture \
  --bundle-id com.kairi7011.sumi.livepromo \
  --output test-artifacts/demo-window.mov --duration 65 --fps 30 \
  --report test-artifacts/demo-window.json
```

録画はすぐに始まり、`recording started`と表示されます。指定した時間が経つと停止します。開始・保存完了の通知を受けた実時刻をISO8601形式でログに出し、録画ファイルのSHA-256ハッシュとともにJSONにも記録します。実際のカーソルも映りますが、`--hide-cursor`で除外できます。システム音声、マイク音声、クリックの装飾は入りません。選んだウィンドウをRetinaのピクセル数で収録し、デスクトップや無関係なウィンドウは含めません。録画中はウィンドウのサイズを変えないでください。複数のウィンドウが見つかった場合は、`--list`でIDを確認し、`--window-id ID`を追加します。一覧にはIDとサイズだけを出し、ウィンドウのタイトルは出しません。

fpsは要求する撮影間隔の指定であり、実際にその枚数が届くことを保証するものではありません。保存された映像を確認し、読み取り検証や前後の切り取りには`process-recording.swift`を使ってください。録画にはAppleの[ScreenCaptureKitの録画出力](https://developer.apple.com/documentation/screencapturekit/screcordingoutput)を使っています。

## 従来の静止画カード

`render-promo.swift`と`storyboard.json`は、スクリーンショットを使う紹介カード用に残しています。そこで作る静止画の連続表示は、操作を録画した映像ではありません。実際の操作を紹介する動画には、録画したMOVと`process-recording.swift`を使ってください。
