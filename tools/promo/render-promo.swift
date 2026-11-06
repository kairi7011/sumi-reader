import AppKit
import AVFoundation
import CoreGraphics
import CoreVideo
import Foundation
import ImageIO
import UniformTypeIdentifiers

// Usage: swift render-promo.swift storyboard.json --output demo.mp4 --cards cards/
// A storyboard contains title, subtitle, optional transition, and scenes with
// image, heading, caption, duration, optional card, layout (aside/focus), and cropRect.
// cropRect is measured in source pixels from the image's top-left corner.
// --self-test DIRECTORY creates clearly marked synthetic grids, never product screenshots.

struct Crop: Codable { let x: Double; let y: Double; let width: Double; let height: Double }
struct Scene: Codable {
    let image: String
    let heading: String
    let caption: String
    var duration: Double = 5
    var card: Bool? = nil
    var layout: String? = nil
    var cropRect: Crop? = nil

    enum CodingKeys: String, CodingKey { case image, heading, caption, duration, card, layout, cropRect }
    init(image: String, heading: String, caption: String, duration: Double, card: Bool? = nil, layout: String? = nil, cropRect: Crop? = nil) {
        self.image = image; self.heading = heading; self.caption = caption; self.duration = duration
        self.card = card; self.layout = layout; self.cropRect = cropRect
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        image = try c.decode(String.self, forKey: .image)
        heading = try c.decode(String.self, forKey: .heading)
        caption = try c.decode(String.self, forKey: .caption)
        duration = try c.decodeIfPresent(Double.self, forKey: .duration) ?? 5
        card = try c.decodeIfPresent(Bool.self, forKey: .card)
        layout = try c.decodeIfPresent(String.self, forKey: .layout)
        cropRect = try c.decodeIfPresent(Crop.self, forKey: .cropRect)
    }
}
struct Storyboard: Codable {
    let title: String
    let subtitle: String
    let scenes: [Scene]
    var transition: Double? = nil
}
struct RenderError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

let width = 1920
let height = 1080
let fps: Int32 = 30
let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
let background = NSColor(srgbRed: 0.953, green: 0.949, blue: 0.929, alpha: 1)
let ink = NSColor(srgbRed: 0.12, green: 0.16, blue: 0.16, alpha: 1)
let mutedInk = NSColor(srgbRed: 0.36, green: 0.40, blue: 0.39, alpha: 1)
let accent = NSColor(srgbRed: 0.30, green: 0.44, blue: 0.39, alpha: 1)

func bitmap(_ w: Int = width, _ h: Int = height) throws -> CGContext {
    guard let context = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8,
                                  bytesPerRow: w * 4, space: colorSpace,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else {
        throw RenderError("Could not allocate a drawing surface.")
    }
    context.interpolationQuality = .high
    return context
}

func withGraphics(_ context: CGContext, _ body: () throws -> Void) rethrows {
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
    defer { NSGraphicsContext.restoreGraphicsState() }
    try body()
}

func text(_ value: String, in rect: CGRect, size: CGFloat, weight: Bool = false, color: NSColor = ink) throws {
    let paragraph = NSMutableParagraphStyle()
    paragraph.lineBreakMode = .byWordWrapping
    paragraph.lineSpacing = size * 0.20
    var chosen = size
    var attributes: [NSAttributedString.Key: Any] = [:]
    while true {
        let font = NSFont(name: weight ? "HiraginoSans-W6" : "HiraginoSans-W3", size: chosen)
            ?? NSFont.systemFont(ofSize: chosen, weight: weight ? .semibold : .regular)
        attributes = [.font: font, .foregroundColor: color, .paragraphStyle: paragraph]
        let bounds = (value as NSString).boundingRect(with: CGSize(width: rect.width, height: .greatestFiniteMagnitude),
                                                      options: [.usesLineFragmentOrigin, .usesFontLeading], attributes: attributes)
        if bounds.height <= rect.height + 1 { break }
        chosen -= 1
        if chosen < size * 0.75 { throw RenderError("A caption is too long for its layout; shorten it before rendering.") }
    }
    (value as NSString).draw(with: rect, options: [.usesLineFragmentOrigin, .usesFontLeading], attributes: attributes)
}

func loadImage(_ url: URL, crop: Crop?) throws -> CGImage {
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
          let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
          let w = properties[kCGImagePropertyPixelWidth] as? Int,
          let h = properties[kCGImagePropertyPixelHeight] as? Int,
          w > 0, h > 0, w <= 16000, h <= 16000, w * h <= 50_000_000,
          let image = CGImageSourceCreateImageAtIndex(source, 0, [kCGImageSourceShouldCacheImmediately: true] as CFDictionary) else {
        throw RenderError("Could not read a bounded screenshot at \(url.lastPathComponent).")
    }
    guard let crop else { return image }
    let rect = CGRect(x: crop.x, y: crop.y, width: crop.width, height: crop.height)
    guard [crop.x, crop.y, crop.width, crop.height].allSatisfy({ $0.isFinite }),
          rect.width >= 1, rect.height >= 1,
          CGRect(x: 0, y: 0, width: w, height: h).contains(rect),
          let cropped = image.cropping(to: rect.integral) else {
        throw RenderError("cropRect must be inside the source screenshot, in top-left pixel coordinates.")
    }
    return cropped
}

func contain(_ image: CGImage, in area: CGRect) -> CGRect {
    let scale = min(area.width / CGFloat(image.width), area.height / CGFloat(image.height))
    let size = CGSize(width: CGFloat(image.width) * scale, height: CGFloat(image.height) * scale)
    return CGRect(x: area.midX - size.width / 2, y: area.midY - size.height / 2, width: size.width, height: size.height)
}

func compose(_ scene: Scene, story: Storyboard, directory: URL) throws -> CGImage {
    let imageURL = URL(fileURLWithPath: scene.image, relativeTo: directory).standardizedFileURL
    let screenshot = try loadImage(imageURL, crop: scene.cropRect)
    let context = try bitmap()
    context.setFillColor(background.cgColor)
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    let focus = scene.layout == "focus"
    let area = focus ? CGRect(x: 96, y: 62, width: 1728, height: 760) : CGRect(x: 500, y: 48, width: 1360, height: 984)
    let frame = contain(screenshot, in: area)
    context.saveGState()
    context.setShadow(offset: CGSize(width: 0, height: -10), blur: 26, color: NSColor.black.withAlphaComponent(0.15).cgColor)
    context.setFillColor(NSColor.white.cgColor)
    context.fill(frame)
    context.restoreGState()
    context.draw(screenshot, in: frame)
    context.setStrokeColor(NSColor.black.withAlphaComponent(0.08).cgColor)
    context.setLineWidth(1)
    context.stroke(frame.insetBy(dx: -0.5, dy: -0.5))
    try withGraphics(context) {
        if focus {
            try text(story.title, in: CGRect(x: 96, y: 1000, width: 1728, height: 30), size: 22, weight: true, color: accent)
            try text(scene.heading, in: CGRect(x: 96, y: 916, width: 1728, height: 72), size: 48, weight: true)
            try text(scene.caption, in: CGRect(x: 96, y: 848, width: 1728, height: 58), size: 26, color: mutedInk)
        } else {
            try text(story.title, in: CGRect(x: 72, y: 890, width: 360, height: 48), size: 30, weight: true, color: accent)
            try text(scene.heading, in: CGRect(x: 72, y: 580, width: 360, height: 240), size: 52, weight: true)
            try text(scene.caption, in: CGRect(x: 72, y: 390, width: 360, height: 160), size: 25, color: mutedInk)
            try text(story.subtitle, in: CGRect(x: 72, y: 52, width: 360, height: 60), size: 18, color: mutedInk)
        }
    }
    guard let composed = context.makeImage() else { throw RenderError("Could not compose a promo frame.") }
    return composed
}

func savePNG(_ image: CGImage, to url: URL) throws {
    guard let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else {
        throw RenderError("Could not create \(url.lastPathComponent).")
    }
    CGImageDestinationAddImage(destination, image, nil)
    guard CGImageDestinationFinalize(destination) else { throw RenderError("Could not finish \(url.lastPathComponent).") }
}

func encode(_ frames: [CGImage], story: Storyboard, to output: URL) throws -> Int {
    guard !FileManager.default.fileExists(atPath: output.path) else {
        throw RenderError("The video output already exists; choose a new path or remove it explicitly.")
    }
    let writer = try AVAssetWriter(outputURL: output, fileType: .mp4)
    writer.shouldOptimizeForNetworkUse = true
    let settings: [String: Any] = [
        AVVideoCodecKey: AVVideoCodecType.h264,
        AVVideoWidthKey: width, AVVideoHeightKey: height,
        AVVideoCompressionPropertiesKey: [
            AVVideoAverageBitRateKey: 10_000_000,
            AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel,
            AVVideoExpectedSourceFrameRateKey: fps,
            AVVideoMaxKeyFrameIntervalKey: fps * 2,
        ],
        AVVideoColorPropertiesKey: [
            AVVideoColorPrimariesKey: AVVideoColorPrimaries_ITU_R_709_2,
            AVVideoTransferFunctionKey: AVVideoTransferFunction_ITU_R_709_2,
            AVVideoYCbCrMatrixKey: AVVideoYCbCrMatrix_ITU_R_709_2,
        ],
    ]
    let input = AVAssetWriterInput(mediaType: .video, outputSettings: settings)
    input.expectsMediaDataInRealTime = false
    let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
        kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
        kCVPixelBufferWidthKey as String: width,
        kCVPixelBufferHeightKey as String: height,
        kCVPixelBufferCGImageCompatibilityKey as String: true,
        kCVPixelBufferCGBitmapContextCompatibilityKey as String: true,
    ])
    guard writer.canAdd(input) else { throw RenderError("The system H.264 encoder is unavailable.") }
    writer.add(input)
    guard writer.startWriting() else { throw writer.error ?? RenderError("Could not start the video writer.") }
    writer.startSession(atSourceTime: .zero)
    let sceneFrames = story.scenes.map { Int(($0.duration * Double(fps)).rounded()) }
    let totalFrames = sceneFrames.reduce(0, +)
    var scene = 0
    var sceneStart = 0
    var lastAppend = ProcessInfo.processInfo.systemUptime
    for index in 0..<totalFrames {
        while !input.isReadyForMoreMediaData {
            if writer.status == .failed || ProcessInfo.processInfo.systemUptime - lastAppend > 30 {
                writer.cancelWriting()
                throw writer.error ?? RenderError("The video encoder stopped accepting frames.")
            }
            Thread.sleep(forTimeInterval: 0.002)
        }
        while index >= sceneStart + sceneFrames[scene] {
            sceneStart += sceneFrames[scene]
            scene += 1
        }
        try autoreleasepool {
            guard let pool = adaptor.pixelBufferPool else { throw RenderError("The video pixel-buffer pool is unavailable.") }
            var allocated: CVPixelBuffer?
            guard CVPixelBufferPoolCreatePixelBuffer(kCFAllocatorDefault, pool, &allocated) == kCVReturnSuccess,
                  let buffer = allocated else { throw RenderError("Could not allocate a video frame.") }
            CVPixelBufferLockBaseAddress(buffer, [])
            defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
            guard let context = CGContext(data: CVPixelBufferGetBaseAddress(buffer), width: width, height: height,
                                          bitsPerComponent: 8, bytesPerRow: CVPixelBufferGetBytesPerRow(buffer), space: colorSpace,
                                          bitmapInfo: CGBitmapInfo.byteOrder32Little.rawValue | CGImageAlphaInfo.premultipliedFirst.rawValue) else {
                throw RenderError("Could not draw a video frame.")
            }
            let canvas = CGRect(x: 0, y: 0, width: width, height: height)
            context.draw(frames[scene], in: canvas)
            if scene + 1 < frames.count {
                let fade = min(story.transition ?? 0.45, story.scenes[scene].duration / 4, story.scenes[scene + 1].duration / 4)
                let remaining = Double(sceneStart + sceneFrames[scene] - index - 1) / Double(fps)
                if fade > 0 && remaining < fade {
                    let t = min(1, max(0, 1 - remaining / fade))
                    context.setAlpha(t * t * (3 - 2 * t))
                    context.draw(frames[scene + 1], in: canvas)
                }
            }
            guard adaptor.append(buffer, withPresentationTime: CMTime(value: Int64(index), timescale: fps)) else {
                throw writer.error ?? RenderError("Could not append a video frame.")
            }
        }
        lastAppend = ProcessInfo.processInfo.systemUptime
    }
    writer.endSession(atSourceTime: CMTime(value: Int64(totalFrames), timescale: fps))
    input.markAsFinished()
    let finished = DispatchSemaphore(value: 0)
    writer.finishWriting { finished.signal() }
    guard finished.wait(timeout: .now() + 30) == .success, writer.status == .completed else {
        writer.cancelWriting()
        throw writer.error ?? RenderError("The video writer did not finish successfully.")
    }
    return totalFrames
}

func selfTest(at directory: URL) throws -> URL {
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    for index in 0..<3 {
        let context = try bitmap(2320, 1600)
        let colors: [NSColor] = [.systemRed, .systemGreen, .systemBlue, .systemYellow]
        for quadrant in 0..<4 {
            context.setFillColor(colors[(quadrant + index) % 4].cgColor)
            context.fill(CGRect(x: (quadrant % 2) * 1160, y: (quadrant / 2) * 800, width: 1160, height: 800))
        }
        context.setStrokeColor(NSColor.white.withAlphaComponent(0.4).cgColor)
        context.setLineWidth(2)
        for x in stride(from: 0, through: 2320, by: 80) { context.move(to: CGPoint(x: x, y: 0)); context.addLine(to: CGPoint(x: x, y: 1600)) }
        for y in stride(from: 0, through: 1600, by: 80) { context.move(to: CGPoint(x: 0, y: y)); context.addLine(to: CGPoint(x: 2320, y: y)) }
        context.strokePath()
        try withGraphics(context) {
            try text("SYNTHETIC ENCODER TEST — NOT PRODUCT UI", in: CGRect(x: 80, y: 1350, width: 2160, height: 150), size: 64, weight: true, color: .white)
            try text("TOP LEFT", in: CGRect(x: 80, y: 1160, width: 900, height: 120), size: 60, weight: true, color: .white)
            try text("BOTTOM RIGHT", in: CGRect(x: 1240, y: 80, width: 1000, height: 120), size: 60, weight: true, color: .black)
        }
        try savePNG(context.makeImage()!, to: directory.appendingPathComponent("synthetic-\(index + 1).png"))
    }
    let story = Storyboard(title: "Sumi · TEST", subtitle: "Synthetic rendering validation", scenes: [
        Scene(image: "synthetic-1.png", heading: "日本語の\n見え方を確認", caption: "これはエンコーダーの検証用画像です。実際のアプリ画面ではありません。", duration: 8, card: true),
        Scene(image: "synthetic-2.png", heading: "画像全体を保つ", caption: "縦横比を変えずに配置し、穏やかな切り替えを確認します。", duration: 8, card: true),
        Scene(image: "synthetic-3.png", heading: "切り出した範囲の確認", caption: "元画像の左上の領域。検証用であり、公開用の素材ではありません。", duration: 8, card: true, layout: "focus", cropRect: Crop(x: 0, y: 0, width: 1160, height: 800)),
    ], transition: 0.45)
    let output = directory.appendingPathComponent("synthetic-storyboard.json")
    let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try encoder.encode(story).write(to: output)
    return output
}

func main() throws {
    let arguments = Array(CommandLine.arguments.dropFirst())
    guard let first = arguments.first else { throw RenderError("Usage: render-promo.swift storyboard.json --output demo.mp4 --cards cards/ [--cards-only]") }
    func option(_ key: String) throws -> String? {
        guard let index = arguments.firstIndex(of: key) else { return nil }
        guard index + 1 < arguments.count, !arguments[index + 1].hasPrefix("--") else { throw RenderError("\(key) needs a path.") }
        return arguments[index + 1]
    }
    let storyboardURL: URL
    if first == "--self-test" {
        guard let path = try option("--self-test") else { throw RenderError("--self-test needs a directory.") }
        storyboardURL = try selfTest(at: URL(fileURLWithPath: path, isDirectory: true))
    } else { storyboardURL = URL(fileURLWithPath: first).standardizedFileURL }
    let directory = storyboardURL.deletingLastPathComponent()
    let story = try JSONDecoder().decode(Storyboard.self, from: Data(contentsOf: storyboardURL))
    guard !story.scenes.isEmpty, story.scenes.count <= 16,
          story.scenes.allSatisfy({ $0.duration.isFinite && $0.duration >= 0.5 && $0.duration <= 30 && [nil, "aside", "focus"].contains($0.layout) }),
          story.scenes.reduce(0, { $0 + $1.duration }) <= 300,
          (story.transition ?? 0.45).isFinite, (story.transition ?? 0.45) >= 0, (story.transition ?? 0.45) <= 2 else {
        throw RenderError("Use 1–16 scenes, 0.5–30 seconds each, at most 300 seconds total, and aside/focus layouts.")
    }
    let movie = try option("--output").map { URL(fileURLWithPath: $0) } ?? directory.appendingPathComponent("promo.mp4")
    let cards = try option("--cards").map { URL(fileURLWithPath: $0, isDirectory: true) } ?? directory.appendingPathComponent("cards", isDirectory: true)
    try FileManager.default.createDirectory(at: cards, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: movie.deletingLastPathComponent(), withIntermediateDirectories: true)
    var frames: [CGImage] = []
    var cardNames: [String] = []
    let explicitCards = story.scenes.contains { $0.card != nil }
    for (index, scene) in story.scenes.enumerated() {
        let frame = try autoreleasepool { try compose(scene, story: story, directory: directory) }
        frames.append(frame)
        if explicitCards ? scene.card == true : index < 3 {
            let name = String(format: "promo-%02d.png", index + 1)
            try savePNG(frame, to: cards.appendingPathComponent(name))
            cardNames.append(name)
        }
    }
    let count = arguments.contains("--cards-only") ? 0 : try encode(frames, story: story, to: movie)
    let report: [String: Any] = ["width": width, "height": height, "fps": fps, "frames": count,
                               "duration": Double(count) / Double(fps), "cards": cardNames,
                               "video": count == 0 ? NSNull() : movie.lastPathComponent,
                               "format": "H.264 MP4; captioned screenshot walkthrough; no audio"]
    let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
    FileHandle.standardOutput.write(data)
    print()
}

do { try main() }
catch { FileHandle.standardError.write(Data(("render-promo: \(error)\n").utf8)); exit(1) }
