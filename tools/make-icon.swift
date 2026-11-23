// Renders the Engram app icon into an .iconset directory.
// Usage: swift tools/make-icon.swift <out.iconset> && iconutil -c icns <out.iconset>
import AppKit

let out = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? "AppIcon.iconset")
try? FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)

func render(_ px: Int) -> Data {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8, samplesPerPixel: 4,
        hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
    )!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let s = CGFloat(px)
    // macOS icon grid: 824/1024 body with ~185/1024 corner radius.
    let inset = s * 100 / 1024
    let body = NSRect(x: inset, y: inset, width: s - 2 * inset, height: s - 2 * inset)
    let path = NSBezierPath(roundedRect: body, xRadius: s * 185 / 1024, yRadius: s * 185 / 1024)
    NSGradient(colors: [
        NSColor(red: 0.27, green: 0.29, blue: 0.86, alpha: 1),
        NSColor(red: 0.47, green: 0.36, blue: 0.97, alpha: 1),
    ])!.draw(in: path, angle: 90)

    // Graph motif: three linked nodes.
    let nodes = [CGPoint(x: 0.5, y: 0.70), CGPoint(x: 0.31, y: 0.36), CGPoint(x: 0.69, y: 0.36)]
        .map { CGPoint(x: $0.x * s, y: $0.y * s) }
    let edges = NSBezierPath()
    edges.lineWidth = s * 0.028
    edges.lineCapStyle = .round
    for (a, b) in [(0, 1), (0, 2), (1, 2)] {
        edges.move(to: nodes[a])
        edges.line(to: nodes[b])
    }
    NSColor.white.withAlphaComponent(0.55).setStroke()
    edges.stroke()
    for (i, p) in nodes.enumerated() {
        let r = s * (i == 0 ? 0.105 : 0.085)
        let dot = NSBezierPath(ovalIn: NSRect(x: p.x - r, y: p.y - r, width: 2 * r, height: 2 * r))
        NSColor.white.setFill()
        dot.fill()
        let core = r * 0.45
        NSColor(red: 0.37, green: 0.33, blue: 0.93, alpha: 1).setFill()
        NSBezierPath(ovalIn: NSRect(x: p.x - core, y: p.y - core, width: 2 * core, height: 2 * core)).fill()
    }
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

for base in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let name = scale == 1 ? "icon_\(base)x\(base).png" : "icon_\(base)x\(base)@2x.png"
        try render(base * scale).write(to: out.appendingPathComponent(name))
    }
}
print("wrote \(out.path)")
