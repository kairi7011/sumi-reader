import AVFoundation
import CoreGraphics
import CoreVideo
import CryptoKit
import Foundation
import ImageIO
import UniformTypeIdentifiers

struct RecordingError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

func digest(_ url: URL) throws -> String {
    let file = try FileHandle(forReadingFrom: url)
    defer { try? file.close() }
    var hash = SHA256()
    while let chunk = try file.read(upToCount: 1_048_576), !chunk.isEmpty { hash.update(data: chunk) }
    return hash.finalize().map { String(format: "%02x", $0) }.joined()
}

struct Options {
    let input: URL
    let output: URL
    var start: Double?
    var end: Double?
    var crop: CGRect?
    var width: Double?
    var height: Double?
    var mute = false
    var previews: URL?
    var report: URL?

    static func parse() throws -> Options {
        let args = Array(CommandLine.arguments.dropFirst())
        guard let source = args.first, !source.hasPrefix("--") else {
            throw RecordingError("Usage: process-recording.swift INPUT.mov --output OUTPUT.mp4 [--start SECONDS --end SECONDS] [--crop X,Y,WIDTH,HEIGHT] [--width PIXELS --height PIXELS] [--mute] [--previews DIRECTORY] [--report FILE.json]")
        }
        var values: [String: String] = [:]
        var mute = false
        var index = 1
        let keys: Set<String> = ["--output", "--start", "--end", "--crop", "--width", "--height", "--previews", "--report"]
        while index < args.count {
            let key = args[index]
            if key == "--mute" { mute = true; index += 1; continue }
            guard keys.contains(key), index + 1 < args.count, !args[index + 1].hasPrefix("--"), values[key] == nil else {
                throw RecordingError("Unknown, repeated, or incomplete option: \(key)")
            }
            values[key] = args[index + 1]
            index += 2
        }
        guard let destination = values["--output"] else { throw RecordingError("--output needs an MP4 destination.") }
        var options = Options(input: URL(fileURLWithPath: source).standardizedFileURL,
                              output: URL(fileURLWithPath: destination).standardizedFileURL)
        func number(_ key: String) throws -> Double? {
            guard let raw = values[key] else { return nil }
            guard let result = Double(raw), result.isFinite, result >= 0 else { throw RecordingError("\(key) needs a finite nonnegative number.") }
            return result
        }
        options.start = try number("--start")
        options.end = try number("--end")
        options.width = try number("--width")
        options.height = try number("--height")
        for dimension in [options.width, options.height].compactMap({ $0 }) {
            guard dimension >= 2, dimension <= 8192 else { throw RecordingError("Output bounds must be between 2 and 8192 pixels.") }
        }
        if let raw = values["--crop"] {
            let parts = raw.split(separator: ",", omittingEmptySubsequences: false).compactMap { Double($0) }
            guard parts.count == 4, parts.allSatisfy({ $0.isFinite }), parts[0] >= 0, parts[1] >= 0, parts[2] >= 2, parts[3] >= 2 else {
                throw RecordingError("--crop needs X,Y,WIDTH,HEIGHT in top-left displayed-video pixels.")
            }
            options.crop = CGRect(x: parts[0], y: parts[1], width: parts[2], height: parts[3])
        }
        options.mute = mute
        options.previews = values["--previews"].map { URL(fileURLWithPath: $0, isDirectory: true) }
        options.report = values["--report"].map { URL(fileURLWithPath: $0) }
        guard options.input != options.output, options.output.pathExtension.lowercased() == "mp4" else {
            throw RecordingError("Choose a separate .mp4 output; the original recording is never overwritten.")
        }
        guard !FileManager.default.fileExists(atPath: options.output.path) else { throw RecordingError("The output already exists. Choose a new filename.") }
        return options
    }
}

func savePNG(_ image: CGImage, at url: URL) throws {
    guard let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else {
        throw RecordingError("Could not create a preview image.")
    }
    CGImageDestinationAddImage(destination, image, nil)
    guard CGImageDestinationFinalize(destination) else { throw RecordingError("Could not finish a preview image.") }
}

func verify(_ url: URL, expectedDuration: Double, expectedSize: CGSize, previews: URL?) async throws -> [String: Any] {
    let asset = AVURLAsset(url: url)
    let videos = try await asset.loadTracks(withMediaType: .video)
    guard videos.count == 1, let track = videos.first else { throw RecordingError("The output must contain one video track.") }
    let audio = try await asset.loadTracks(withMediaType: .audio)
    let duration = try await asset.load(.duration)
    let size = try await track.load(.naturalSize)
    let rate = try await track.load(.nominalFrameRate)
    let descriptions = try await track.load(.formatDescriptions)
    let subtype = descriptions.first.map { CMFormatDescriptionGetMediaSubType($0) } ?? 0
    let codec = String(bytes: [24, 16, 8, 0].map { UInt8((subtype >> $0) & 255) }, encoding: .ascii) ?? "unknown"
    guard codec == "avc1", size == expectedSize,
          abs(duration.seconds - expectedDuration) < 0.02 else {
        throw RecordingError("The exported codec, size, or duration differs from the requested recording segment.")
    }
    let reader = try AVAssetReader(asset: asset)
    let output = AVAssetReaderTrackOutput(track: track, outputSettings: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
    reader.add(output)
    guard reader.startReading() else { throw reader.error ?? RecordingError("Could not decode the exported recording.") }
    var frames = 0
    var previous = -Double.infinity
    var minimumInterval = Double.infinity
    var maximumInterval = 0.0
    while let time = autoreleasepool(invoking: { output.copyNextSampleBuffer().map { CMSampleBufferGetPresentationTimeStamp($0).seconds } }) {
        guard time.isFinite, time > previous, time >= -0.001, time < duration.seconds + 0.001 else {
            throw RecordingError("The exported recording contains an invalid frame timestamp.")
        }
        if frames > 0 {
            minimumInterval = min(minimumInterval, time - previous)
            maximumInterval = max(maximumInterval, time - previous)
        }
        previous = time
        frames += 1
    }
    guard reader.status == .completed, frames > 0 else { throw reader.error ?? RecordingError("The recording did not fully decode.") }
    if let previews {
        try FileManager.default.createDirectory(at: previews, withIntermediateDirectories: true)
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.requestedTimeToleranceBefore = .zero
        generator.requestedTimeToleranceAfter = .zero
        for (index, seconds) in [0, duration.seconds / 2, max(0, duration.seconds - 0.1)].enumerated() {
            let frame = try await generator.image(at: CMTime(seconds: seconds, preferredTimescale: 600))
            try savePNG(frame.image, at: previews.appendingPathComponent("recording-frame-\(index + 1).png"))
        }
    }
    return ["file": url.lastPathComponent, "codec": codec, "width": size.width, "height": size.height,
            "seconds": duration.seconds, "nominal_fps": rate, "decoded_frames": frames,
            "average_decoded_frames_per_second": Double(frames) / duration.seconds,
            "frame_interval_min_seconds": frames > 1 ? minimumInterval as Any : NSNull(),
            "frame_interval_max_seconds": frames > 1 ? maximumInterval as Any : NSNull(),
            "audio_tracks": audio.count, "decode_completed": true, "timestamps_increasing": true]
}

func main() async throws {
    let options = try Options.parse()
    let source = AVURLAsset(url: options.input)
    let videoTracks = try await source.loadTracks(withMediaType: .video)
    guard videoTracks.count == 1, let sourceTrack = videoTracks.first else { throw RecordingError("Provide a recording with exactly one video track.") }
    let duration = try await source.load(.duration)
    let naturalSize = try await sourceTrack.load(.naturalSize)
    let preferred = try await sourceTrack.load(.preferredTransform)
    let nominalRate = try await sourceTrack.load(.nominalFrameRate)
    let trackRange = try await sourceTrack.load(.timeRange)
    let start = options.start ?? max(0, trackRange.start.seconds)
    let end = options.end ?? min(duration.seconds, trackRange.end.seconds)
    guard duration.seconds.isFinite, start >= max(0, trackRange.start.seconds),
          end <= duration.seconds + 0.001, end <= trackRange.end.seconds + 0.001, end > start else {
        throw RecordingError("The requested start/end must lie within the source video track.")
    }
    let range = CMTimeRange(start: CMTime(seconds: start, preferredTimescale: 60000),
                            end: CMTime(seconds: end, preferredTimescale: 60000))
    let rawBounds = CGRect(origin: .zero, size: naturalSize)
    let orientedBounds = rawBounds.applying(preferred).standardized
    let displayBounds = CGRect(origin: .zero, size: orientedBounds.size)
    let crop = options.crop ?? displayBounds
    guard displayBounds.contains(crop), naturalSize.width > 0, naturalSize.height > 0 else {
        throw RecordingError("The crop must stay inside the displayed recording.")
    }
    var scale = 1.0
    if let width = options.width { scale = min(scale, width / crop.width) }
    if let height = options.height { scale = min(scale, height / crop.height) }
    let renderSize = CGSize(width: max(2, floor(crop.width * scale / 2) * 2),
                            height: max(2, floor(crop.height * scale / 2) * 2))
    guard renderSize.width <= 8192, renderSize.height <= 8192 else { throw RecordingError("Use --width or --height to reduce a recording larger than 8192 pixels.") }
    let normalized = preferred.concatenating(CGAffineTransform(translationX: -orientedBounds.minX, y: -orientedBounds.minY))
    let sourceCrop = crop.applying(normalized.inverted()).standardized.intersection(rawBounds)
    let fit = min(renderSize.width / crop.width, renderSize.height / crop.height)
    let transform = normalized
        .concatenating(CGAffineTransform(translationX: -crop.minX, y: -crop.minY))
        .concatenating(CGAffineTransform(scaleX: fit, y: fit))
        .concatenating(CGAffineTransform(translationX: (renderSize.width - crop.width * fit) / 2,
                                         y: (renderSize.height - crop.height * fit) / 2))

    let composition = AVMutableComposition()
    guard let track = composition.addMutableTrack(withMediaType: .video, preferredTrackID: kCMPersistentTrackID_Invalid) else {
        throw RecordingError("Could not create the recording track.")
    }
    try track.insertTimeRange(range, of: sourceTrack, at: .zero)
    track.preferredTransform = .identity
    if !options.mute {
        for sourceAudio in try await source.loadTracks(withMediaType: .audio) {
            let audioRange = try await sourceAudio.load(.timeRange)
            let intersection = CMTimeRangeGetIntersection(range, otherRange: audioRange)
            if intersection.duration.seconds <= 0 { continue }
            guard let target = composition.addMutableTrack(withMediaType: .audio, preferredTrackID: kCMPersistentTrackID_Invalid) else {
                throw RecordingError("Could not retain a recording audio track.")
            }
            try target.insertTimeRange(intersection, of: sourceAudio, at: CMTimeSubtract(intersection.start, range.start))
        }
    }
    let layer = AVMutableVideoCompositionLayerInstruction(assetTrack: track)
    layer.setCropRectangle(sourceCrop, at: .zero)
    layer.setTransform(transform, at: .zero)
    let instruction = AVMutableVideoCompositionInstruction()
    instruction.timeRange = CMTimeRange(start: .zero, duration: range.duration)
    instruction.layerInstructions = [layer]
    let videoComposition = AVMutableVideoComposition()
    videoComposition.instructions = [instruction]
    videoComposition.renderSize = renderSize
    videoComposition.frameDuration = CMTime(seconds: 1 / Double(min(60, max(nominalRate, 30))), preferredTimescale: 60000)
    // Screen recordings may be variable-rate; preserve their actual timing instead of synthesizing a fixed-rate slideshow.
    videoComposition.sourceTrackIDForFrameTiming = track.trackID
    guard let exporter = AVAssetExportSession(asset: composition, presetName: AVAssetExportPresetHighestQuality) else {
        throw RecordingError("The system H.264 exporter is unavailable.")
    }
    exporter.videoComposition = videoComposition
    exporter.timeRange = CMTimeRange(start: .zero, duration: range.duration)
    exporter.metadata = []
    exporter.shouldOptimizeForNetworkUse = true
    try FileManager.default.createDirectory(at: options.output.deletingLastPathComponent(), withIntermediateDirectories: true)
    try await exporter.export(to: options.output, as: .mp4)
    var report = try await verify(options.output, expectedDuration: range.duration.seconds, expectedSize: renderSize, previews: options.previews)
    report["source_file"] = options.input.lastPathComponent
    report["source_sha256"] = try digest(options.input)
    report["output_sha256"] = try digest(options.output)
    report["source_start_seconds"] = start
    report["source_end_seconds"] = end
    report["source_display_size"] = [displayBounds.width, displayBounds.height]
    report["crop_top_left_pixels"] = [crop.minX, crop.minY, crop.width, crop.height]
    report["speed"] = 1
    report["processing"] = "Continuous live footage: trim, fixed crop, proportional resize, H.264 encoding. No generated UI, freeze frames, transitions, captions, or speed changes."
    let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
    if let destination = options.report {
        try FileManager.default.createDirectory(at: destination.deletingLastPathComponent(), withIntermediateDirectories: true)
        try data.write(to: destination)
    }
    FileHandle.standardOutput.write(data)
    print()
}

do { try await main() }
catch { FileHandle.standardError.write(Data(("process-recording: \(error)\n").utf8)); exit(1) }
