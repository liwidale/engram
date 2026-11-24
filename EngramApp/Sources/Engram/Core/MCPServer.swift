import Foundation

enum MCPServer {
    static var detectedPath: String {
        let installed = "/Applications/Engram.app/Contents/MacOS/engram-mcp"
        let bundled = Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/engram-mcp").path
        let fm = FileManager.default
        if Bundle.main.bundlePath.hasPrefix("/Applications/"), fm.isExecutableFile(atPath: bundled) { return bundled }
        if fm.isExecutableFile(atPath: installed) { return installed }
        if fm.isExecutableFile(atPath: bundled) { return bundled }
        return installed
    }

    static var path: String {
        let override = Pref.mcpBinaryOverride
        return override.isEmpty ? detectedPath : (override as NSString).expandingTildeInPath
    }

    enum Check: Equatable {
        case ok(String)
        case failed(String)
    }

    static func verify(_ path: String) async -> Check {
        await Task.detached {
            guard FileManager.default.isExecutableFile(atPath: path) else {
                return .failed(String(localized: "No executable file at this path. Choose the engram-mcp binary."))
            }
            let process = Process()
            process.executableURL = URL(fileURLWithPath: path)
            process.arguments = ["--version"]
            let out = Pipe()
            process.standardOutput = out
            process.standardError = Pipe()
            do {
                try process.run()
            } catch {
                return .failed(error.localizedDescription)
            }
            let deadline = Date().addingTimeInterval(5)
            while process.isRunning, Date() < deadline { try? await Task.sleep(for: .milliseconds(50)) }
            if process.isRunning {
                process.terminate()
                return .failed(String(localized: "The binary did not respond within 5 seconds."))
            }
            let text = String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard process.terminationStatus == 0, text.hasPrefix("engram-mcp") else {
                return .failed(String(localized: "This binary is not engram-mcp."))
            }
            return .ok(text)
        }.value
    }
}
