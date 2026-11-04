import AppKit
import Foundation

let output = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "assets/app-icon.png"
let side = 1024
let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: side, pixelsHigh: side, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
NSGraphicsContext.current?.imageInterpolation = .high

func color(_ red: CGFloat, _ green: CGFloat, _ blue: CGFloat) -> NSColor {
    NSColor(srgbRed: red / 255, green: green / 255, blue: blue / 255, alpha: 1)
}
func rounded(_ rect: NSRect, _ radius: CGFloat, _ fill: NSColor) {
    fill.setFill()
    NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius).fill()
}

rounded(NSRect(x: 52, y: 44, width: 920, height: 920), 207, color(56, 69, 53))
rounded(NSRect(x: 52, y: 60, width: 920, height: 904), 207, color(78, 95, 71))
rounded(NSRect(x: 249, y: 201, width: 528, height: 620), 38, color(38, 52, 34))
rounded(NSRect(x: 237, y: 224, width: 528, height: 620), 38, color(230, 229, 209))
rounded(NSRect(x: 237, y: 240, width: 528, height: 604), 38, color(250, 248, 231))
color(222, 224, 202).setFill()
NSBezierPath(rect: NSRect(x: 290, y: 240, width: 12, height: 604)).fill()
color(143, 154, 120).setFill()
NSBezierPath(roundedRect: NSRect(x: 365, y: 727, width: 130, height: 10), xRadius: 5, yRadius: 5).fill()

let paragraph = NSMutableParagraphStyle()
paragraph.alignment = .left
let attributes: [NSAttributedString.Key: Any] = [.font: NSFont(name: "Georgia", size: 238) ?? NSFont.systemFont(ofSize: 238), .foregroundColor: color(65, 80, 59), .paragraphStyle: paragraph]
NSAttributedString(string: "s.", attributes: attributes).draw(in: NSRect(x: 353, y: 356, width: 350, height: 310))

color(119, 137, 101).setFill()
let bookmark = NSBezierPath()
bookmark.move(to: NSPoint(x: 632, y: 844))
bookmark.line(to: NSPoint(x: 699, y: 844))
bookmark.line(to: NSPoint(x: 699, y: 676))
bookmark.line(to: NSPoint(x: 665.5, y: 701))
bookmark.line(to: NSPoint(x: 632, y: 676))
bookmark.close()
bookmark.fill()

NSGraphicsContext.restoreGraphicsState()
try FileManager.default.createDirectory(at: URL(fileURLWithPath: output).deletingLastPathComponent(), withIntermediateDirectories: true)
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: output))
print("Wrote \(output)")
