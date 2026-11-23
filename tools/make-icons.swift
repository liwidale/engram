import AppKit

let root = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? FileManager.default.currentDirectoryPath)
let fm = FileManager.default
let branding = root.appendingPathComponent("branding")
let icons = branding.appendingPathComponent("icons")
try fm.createDirectory(at: icons, withIntermediateDirectories: true)

let accentTop = NSColor(red: 0.47, green: 0.36, blue: 0.97, alpha: 1)
let accentBottom = NSColor(red: 0.27, green: 0.29, blue: 0.86, alpha: 1)

func bitmap(_ w: Int, _ h: Int, _ draw: (CGFloat, CGFloat) -> Void) -> NSBitmapImageRep {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: w, pixelsHigh: h, bitsPerSample: 8, samplesPerPixel: 4,
        hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
    )!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    draw(CGFloat(w), CGFloat(h))
    NSGraphicsContext.restoreGraphicsState()
    return rep
}

func drawMark(in frame: NSRect) {
    let s = frame.width
    let inset = s * 100 / 1024
    let body = frame.insetBy(dx: inset, dy: inset)
    let tile = NSBezierPath(roundedRect: body, xRadius: s * 185 / 1024, yRadius: s * 185 / 1024)
    NSGradient(colors: [accentBottom, accentTop])!.draw(in: tile, angle: 90)
    let nodes = [CGPoint(x: 0.5, y: 0.70), CGPoint(x: 0.31, y: 0.36), CGPoint(x: 0.69, y: 0.36)]
        .map { CGPoint(x: frame.minX + $0.x * s, y: frame.minY + $0.y * s) }
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
        NSColor.white.setFill()
        NSBezierPath(ovalIn: NSRect(x: p.x - r, y: p.y - r, width: 2 * r, height: 2 * r)).fill()
        let core = r * 0.45
        NSColor(red: 0.37, green: 0.33, blue: 0.93, alpha: 1).setFill()
        NSBezierPath(ovalIn: NSRect(x: p.x - core, y: p.y - core, width: 2 * core, height: 2 * core)).fill()
    }
}

func png(_ rep: NSBitmapImageRep) -> Data { rep.representation(using: .png, properties: [:])! }
func mark(_ px: Int) -> Data { png(bitmap(px, px) { w, h in drawMark(in: NSRect(x: 0, y: 0, width: w, height: h)) }) }

try mark(1024).write(to: branding.appendingPathComponent("logo.png"))

let wordmark = bitmap(1600, 400) { w, h in
    drawMark(in: NSRect(x: 0, y: 0, width: h, height: h))
    let font = NSFont.systemFont(ofSize: 190, weight: .bold)
    let text = NSAttributedString(string: "Engram", attributes: [.font: font, .foregroundColor: accentBottom])
    text.draw(at: NSPoint(x: h + 40, y: (h - text.size().height) / 2))
}
try png(wordmark).write(to: branding.appendingPathComponent("wordmark.png"))

for px in [32, 48, 64, 128, 256, 512] {
    try mark(px).write(to: icons.appendingPathComponent("engram-\(px).png"))
}

let icoSizes = [16, 24, 32, 48, 64, 128, 256]
let payloads = icoSizes.map(mark)
var ico = Data()
func u16(_ v: Int) { ico.append(contentsOf: [UInt8(v & 0xff), UInt8(v >> 8 & 0xff)]) }
func u32(_ v: Int) { u16(v & 0xffff); u16(v >> 16) }
u16(0); u16(1); u16(icoSizes.count)
var offset = 6 + 16 * icoSizes.count
for (size, data) in zip(icoSizes, payloads) {
    ico.append(UInt8(size == 256 ? 0 : size))
    ico.append(UInt8(size == 256 ? 0 : size))
    ico.append(0)
    ico.append(0)
    u16(1); u16(32); u32(data.count); u32(offset)
    offset += data.count
}
payloads.forEach { ico.append($0) }
try ico.write(to: icons.appendingPathComponent("engram.ico"))

let iconset = fm.temporaryDirectory.appendingPathComponent("AppIcon-\(UUID().uuidString).iconset")
try fm.createDirectory(at: iconset, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    try mark(base).write(to: iconset.appendingPathComponent("icon_\(base)x\(base).png"))
    try mark(base * 2).write(to: iconset.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset.path, "-o", root.appendingPathComponent("EngramApp/Support/AppIcon.icns").path]
try iconutil.run()
iconutil.waitUntilExit()
try? fm.removeItem(at: iconset)

let assets = root.appendingPathComponent(".github/assets")
try fm.createDirectory(at: assets, withIntermediateDirectories: true)
try mark(256).write(to: assets.appendingPathComponent("icon.png"))
print("Icons written to \(branding.path)")
