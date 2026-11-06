import AppKit
import AVFoundation
import CoreGraphics
import CryptoKit
import Foundation
import ScreenCaptureKit

struct WindowRecordingError: Error, CustomStringConvertible {
    let description: String
    init(_ message: String) { description = message }
}

func timestamp(_ date: Date) -> String {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter.string(from: date)
}

func digest(_ url: URL) throws -> String {
    let file = try FileHandle(forReadingFrom: url)
    defer { try? file.close() }
    var hash = SHA256()
    while let chunk = try file.read(upToCount: 1_048_576), !chunk.isEmpty { hash.update(data: chunk) }
    return hash.finalize().map { String(format: "%02x", $0) }.joined()
}

@available(macOS 15.0, *)
final class RecordingDelegate: NSObject, SCRecordingOutputDelegate, SCStreamDelegate, @unchecked Sendable {
    private let lock = NSLock()
    private var start: Double?
    private var finished = false
    private var failure: Error?
    private var startedAt: Date?
    private var finishedAt: Date?

    func state() -> (start: Double?, finished: Bool, error: Error?, startedAt: Date?, finishedAt: Date?) {
        lock.lock(); defer { lock.unlock() }
        return (start, finished, failure, startedAt, finishedAt)
    }
    func recordingOutputDidStartRecording(_ output: SCRecordingOutput) {
        let now = Date()
        lock.lock(); start = ProcessInfo.processInfo.systemUptime; startedAt = now; lock.unlock()
        FileHandle.standardError.write(Data("record-window: recording started \(timestamp(now))\n".utf8))
    }
    func recordingOutputDidFinishRecording(_ output: SCRecordingOutput) {
        let now = Date()
        lock.lock(); finished = true; finishedAt = now; lock.unlock()
        FileHandle.standardError.write(Data("record-window: recording finished \(timestamp(now))\n".utf8))
    }
    func recordingOutput(_ output: SCRecordingOutput, didFailWithError error: Error) {
        lock.lock(); failure = error; lock.unlock()
    }
    func stream(_ stream: SCStream, didStopWithError error: Error) {
        lock.lock(); failure = error; lock.unlock()
    }
}

@available(macOS 15.0, *)
@MainActor
func record() async throws {
    let args = Array(CommandLine.arguments.dropFirst())
    var values: [String: String] = [:]
    var flags = Set<String>()
    var index = 0
    while index < args.count {
        let key = args[index]
        if ["--allow-screen-capture", "--list", "--hide-cursor"].contains(key) {
            guard flags.insert(key).inserted else { throw WindowRecordingError("Repeated option: \(key)") }
            index += 1; continue
        }
        guard ["--bundle-id", "--window-id", "--output", "--duration", "--fps", "--report"].contains(key),
              index + 1 < args.count, !args[index + 1].hasPrefix("--"), values[key] == nil else {
            throw WindowRecordingError("Unknown, repeated, or incomplete option: \(key)")
        }
        values[key] = args[index + 1]
        index += 2
    }
    guard flags.contains("--allow-screen-capture"), let bundle = values["--bundle-id"], !bundle.isEmpty else {
        throw WindowRecordingError("Use only after the user authorizes screen capture. Usage: record-window.swift --allow-screen-capture --bundle-id ID [--list | --output FILE.mov --duration SECONDS] [--window-id ID] [--fps 30] [--hide-cursor] [--report FILE.json]")
    }
    let duration = Double(values["--duration"] ?? "65") ?? .nan
    let fps = Int(values["--fps"] ?? "30") ?? 0
    guard duration.isFinite, duration >= 1, duration <= 600, (1...60).contains(fps) else {
        throw WindowRecordingError("Duration must be 1–600 seconds and FPS 1–60.")
    }
    let requestedID = values["--window-id"].flatMap(UInt32.init)
    guard values["--window-id"] == nil || requestedID != nil else { throw WindowRecordingError("--window-id must be an unsigned integer.") }
    let output = values["--output"].map { URL(fileURLWithPath: $0).standardizedFileURL }
    if !flags.contains("--list") {
        guard let output, ["mov", "mp4"].contains(output.pathExtension.lowercased()),
              !FileManager.default.fileExists(atPath: output.path) else {
            throw WindowRecordingError("Choose a new .mov or .mp4 output. Existing files are never overwritten.")
        }
    }

    // A command-line process needs an AppKit connection before ScreenCaptureKit contacts WindowServer.
    let application = NSApplication.shared
    application.setActivationPolicy(.prohibited)
    // Keep enumeration after explicit opt-in: even listing may invoke the system's capture-permission UI.
    let shareable = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
    let matches = shareable.windows.filter {
        $0.owningApplication?.bundleIdentifier == bundle && $0.windowLayer == 0 &&
        $0.frame.width >= 100 && $0.frame.height >= 100 && (requestedID == nil || $0.windowID == requestedID)
    }
    if flags.contains("--list") {
        let result: [[String: Any]] = matches.map {
            ["bundle_id": bundle, "window_id": $0.windowID, "width_points": $0.frame.width, "height_points": $0.frame.height]
        }
        FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys]))
        print(); return
    }
    guard matches.count == 1, let window = matches.first, let output else {
        throw WindowRecordingError("Expected one visible window for the exact bundle ID; found \(matches.count). Use --list, then --window-id to select it.")
    }
    let filter = SCContentFilter(desktopIndependentWindow: window)
    let scale = CGFloat(filter.pointPixelScale)
    let width = Int((window.frame.width * scale / 2).rounded(.down)) * 2
    let height = Int((window.frame.height * scale / 2).rounded(.down)) * 2
    guard width >= 2, height >= 2, width <= 8192, height <= 8192, width * height <= 16_000_000 else {
        throw WindowRecordingError("The selected window must fit within 16 MP and 8192 pixels per side.")
    }
    let configuration = SCStreamConfiguration()
    configuration.width = width
    configuration.height = height
    configuration.minimumFrameInterval = CMTime(value: 1, timescale: Int32(fps))
    configuration.queueDepth = 5
    configuration.captureResolution = .best
    configuration.showsCursor = !flags.contains("--hide-cursor")
    configuration.showMouseClicks = false
    configuration.capturesAudio = false
    configuration.captureMicrophone = false
    configuration.excludesCurrentProcessAudio = true
    configuration.includeChildWindows = true
    configuration.ignoreShadowsSingleWindow = true
    configuration.preservesAspectRatio = true

    let recordingConfiguration = SCRecordingOutputConfiguration()
    recordingConfiguration.outputURL = output
    recordingConfiguration.videoCodecType = .h264
    recordingConfiguration.outputFileType = output.pathExtension.lowercased() == "mov" ? .mov : .mp4
    guard recordingConfiguration.availableVideoCodecTypes.contains(.h264),
          recordingConfiguration.availableOutputFileTypes.contains(recordingConfiguration.outputFileType) else {
        throw WindowRecordingError("The requested H.264 recording format is unavailable on this system.")
    }
    try FileManager.default.createDirectory(at: output.deletingLastPathComponent(), withIntermediateDirectories: true)
    let delegate = RecordingDelegate()
    let stream = SCStream(filter: filter, configuration: configuration, delegate: delegate)
    let recording = SCRecordingOutput(configuration: recordingConfiguration, delegate: delegate)
    try stream.addRecordingOutput(recording)
    try await stream.startCapture()
    do {
        let requestedAt = ProcessInfo.processInfo.systemUptime
        while delegate.state().start == nil {
            if let error = delegate.state().error { throw error }
            guard ProcessInfo.processInfo.systemUptime - requestedAt < 15 else { throw WindowRecordingError("Recording did not start within 15 seconds.") }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        let startedAt = delegate.state().start!
        while ProcessInfo.processInfo.systemUptime - startedAt < duration {
            if let error = delegate.state().error { throw error }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        try await stream.stopCapture()
        let stoppedAt = ProcessInfo.processInfo.systemUptime
        while !delegate.state().finished {
            if let error = delegate.state().error { throw error }
            guard ProcessInfo.processInfo.systemUptime - stoppedAt < 30 else { throw WindowRecordingError("Recording did not finalize within 30 seconds.") }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
    } catch {
        try? await stream.stopCapture()
        throw error
    }
    let asset = AVURLAsset(url: output)
    let videos = try await asset.loadTracks(withMediaType: .video)
    let audio = try await asset.loadTracks(withMediaType: .audio)
    guard videos.count == 1, audio.isEmpty else { throw WindowRecordingError("The recording must contain one video track and no audio.") }
    let result: [String: Any] = [
        "file": output.lastPathComponent, "bundle_id": bundle, "window_id": window.windowID,
        "width": width, "height": height, "requested_fps": fps,
        "requested_seconds": duration, "seconds": try await asset.load(.duration).seconds,
        "audio_tracks": audio.count, "cursor": configuration.showsCursor,
        "started_at": delegate.state().startedAt.map(timestamp) ?? "unknown",
        "finished_at": delegate.state().finishedAt.map(timestamp) ?? "unknown",
        "source_sha256": try digest(output),
        "provenance": "ScreenCaptureKit recording of the explicitly selected app window and its child windows; no audio or synthetic frames.",
    ]
    let data = try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys])
    if let path = values["--report"] {
        let report = URL(fileURLWithPath: path)
        try FileManager.default.createDirectory(at: report.deletingLastPathComponent(), withIntermediateDirectories: true)
        try data.write(to: report)
    }
    FileHandle.standardOutput.write(data)
    print()
}

do {
    if #available(macOS 15.0, *) { try await record() }
    else { throw WindowRecordingError("Native recording requires macOS 15 or later.") }
} catch {
    FileHandle.standardError.write(Data(("record-window: \(error)\n").utf8))
    exit(1)
}
