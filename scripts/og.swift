// Renders the site images into a directory: og.png (1200x630 link preview),
// apple-touch-icon.png (180x180) and favicon.png (32x32).
// Usage: swift scripts/og.swift docs
import AppKit

func c(_ hex: UInt32) -> NSColor {
    NSColor(srgbRed: CGFloat(hex >> 16 & 0xff) / 255, green: CGFloat(hex >> 8 & 0xff) / 255,
            blue: CGFloat(hex & 0xff) / 255, alpha: 1)
}
let violet = c(0x3d1dff), pink = c(0xff2e88), lime = c(0xc8ff1a), ink = c(0x0c0a24)

func draw(_ s: String, _ font: NSFont, _ color: NSColor, x: CGFloat, y: CGFloat) {
    NSAttributedString(string: s, attributes: [.font: font, .foregroundColor: color]).draw(at: NSPoint(x: x, y: y))
}
func size(_ s: String, _ font: NSFont) -> NSSize {
    NSAttributedString(string: s, attributes: [.font: font]).size()
}

// Draws into a W x H pixel bitmap (origin bottom-left) and writes it as PNG.
func render(_ path: String, _ W: Int, _ H: Int, _ body: (CGFloat, CGFloat) -> Void) {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: W, pixelsHigh: H, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    body(CGFloat(W), CGFloat(H))
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
}

let dir = CommandLine.arguments[1]

render("\(dir)/og.png", 1200, 630) { W, H in
    violet.setFill(); NSRect(x: 0, y: 0, width: W, height: H).fill()
    lime.setFill(); NSRect(x: 0, y: 0, width: W, height: 22).fill()
    pink.setFill(); NSRect(x: 0, y: 22, width: W, height: 22).fill()

    let mono = NSFont.monospacedSystemFont(ofSize: 40, weight: .bold)
    draw("fog", mono, .white, x: 72, y: 520)
    let site = "fog.j7y.co"
    draw(site, mono, lime, x: W - 72 - size(site, mono).width, y: 520)

    // Headline: largest size where the longer line fits the content width.
    var pt: CGFloat = 140
    while size("THROUGH A GIST.", .systemFont(ofSize: pt, weight: .black)).width > W - 144 { pt -= 2 }
    let head = NSFont.systemFont(ofSize: pt, weight: .black)
    draw("SEND A FILE", head, .white, x: 66, y: 330)
    draw("THROUGH A GIST.", head, lime, x: 66, y: 330 - pt * 0.95)

    // Terminal pill with lime offset shadow, as on the page.
    let pill = NSRect(x: 72, y: 82, width: 640, height: 72)
    lime.setFill(); NSBezierPath(roundedRect: pill.offsetBy(dx: 8, dy: -8), xRadius: 14, yRadius: 14).fill()
    ink.setFill(); NSBezierPath(roundedRect: pill, xRadius: 14, yRadius: 14).fill()
    let cmd = NSFont.monospacedSystemFont(ofSize: 30, weight: .semibold)
    draw("$", cmd, lime, x: 100, y: 100)
    draw("fog send ./report.pdf", cmd, .white, x: 140, y: 100)
}

// Fog glyph, after the 🌫️ emoji: four staggered capsules, one violet→pink→lime
// gradient across all of them. Coordinates are fractions of the box, from the top.
func fog(in box: NSRect) {
    let bars: [(x: CGFloat, y: CGFloat, w: CGFloat)] =
        [(0.20, 0.02, 0.62), (0.02, 0.28, 0.76), (0.24, 0.54, 0.74), (0.08, 0.80, 0.62)]
    let h = (box.height * 0.19).rounded()
    let path = NSBezierPath()
    for b in bars {
        let r = NSRect(x: (box.minX + b.x * box.width).rounded(),
                       y: (box.maxY - b.y * box.height).rounded() - h,
                       width: (b.w * box.width).rounded(), height: h)
        path.append(NSBezierPath(roundedRect: r, xRadius: h / 2, yRadius: h / 2))
    }
    NSGradient(colors: [violet, pink, lime])!.draw(in: path, angle: -35)
}

// Favicon: transparent, glyph fills the square.
render("\(dir)/favicon.png", 32, 32) { W, H in fog(in: NSRect(x: 0, y: 0, width: W, height: H)) }
// apple-touch-icon must not be transparent (iOS rounds the corners itself): ink square, glyph inset.
render("\(dir)/apple-touch-icon.png", 180, 180) { W, H in
    ink.setFill(); NSRect(x: 0, y: 0, width: W, height: H).fill()
    fog(in: NSRect(x: 0, y: 0, width: W, height: H).insetBy(dx: W * 0.16, dy: H * 0.16))
}
