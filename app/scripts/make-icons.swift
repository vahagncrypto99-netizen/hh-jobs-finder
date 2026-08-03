// Icon generator for hh-jobs.
//
// Draws the marks directly with CoreGraphics instead of rasterizing SVG:
// `qlmanage` renders SVG as a *document thumbnail* and composites it on an
// opaque white page, which turns a menu-bar template icon into a solid white
// square. Here the canvas starts fully transparent and stays that way.
//
//   swift app/scripts/make-icons.swift <output-dir>

import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

// Design canvas: 44×44, top-left origin (the context is flipped below).
let CANVAS: CGFloat = 44

func makeContext(_ px: Int) -> CGContext {
    let ctx = CGContext(
        data: nil, width: px, height: px, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
    ctx.clear(CGRect(x: 0, y: 0, width: px, height: px))
    // Flip to a top-left origin and scale the 44-unit design onto `px`.
    let s = CGFloat(px) / CANVAS
    ctx.translateBy(x: 0, y: CGFloat(px))
    ctx.scaleBy(x: s, y: -s)
    ctx.setLineCap(.round)
    ctx.setLineJoin(.round)
    return ctx
}

func writePNG(_ ctx: CGContext, to url: URL) {
    guard let image = ctx.makeImage(),
          let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)
    else { fatalError("cannot encode \(url.lastPathComponent)") }
    CGImageDestinationAddImage(dest, image, nil)
    CGImageDestinationFinalize(dest)
}

/// One lowercase `h`: a full-height stem, a semicircular shoulder and a
/// half-height right stem. Quarter arcs use the standard circle constant.
func addH(_ ctx: CGContext, x: CGFloat, top: CGFloat, bottom: CGFloat, radius r: CGFloat) {
    let k = r * 0.5523
    let mid = bottom - r * 2.0 // shoulder springs from here

    ctx.move(to: CGPoint(x: x, y: top))
    ctx.addLine(to: CGPoint(x: x, y: bottom))

    ctx.move(to: CGPoint(x: x, y: mid))
    ctx.addCurve(
        to: CGPoint(x: x + r, y: mid - r),
        control1: CGPoint(x: x, y: mid - k),
        control2: CGPoint(x: x + r - k, y: mid - r))
    ctx.addCurve(
        to: CGPoint(x: x + r * 2, y: mid),
        control1: CGPoint(x: x + r + k, y: mid - r),
        control2: CGPoint(x: x + r * 2, y: mid - k))
    ctx.addLine(to: CGPoint(x: x + r * 2, y: bottom))
}

/// A right-pointing arrow: shaft plus chevron head.
func addArrow(_ ctx: CGContext, x1: CGFloat, x2: CGFloat, y: CGFloat, head: CGFloat) {
    ctx.move(to: CGPoint(x: x1, y: y))
    ctx.addLine(to: CGPoint(x: x2, y: y))
    ctx.move(to: CGPoint(x: x2 - head, y: y - head))
    ctx.addLine(to: CGPoint(x: x2, y: y))
    ctx.addLine(to: CGPoint(x: x2 - head, y: y + head))
}

/// Menu-bar mark: the bare `hh` wordmark in pure black on alpha. macOS tints
/// a template icon itself, and an arrow next to it turns to mush at 16 px.
func drawTray(_ px: Int) -> CGContext {
    let ctx = makeContext(px)
    ctx.setStrokeColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
    ctx.setLineWidth(5)
    addH(ctx, x: 7.0, top: 10.5, bottom: 33.5, radius: 6.2)
    addH(ctx, x: 25.6, top: 10.5, bottom: 33.5, radius: 6.2)
    ctx.strokePath()
    return ctx
}

/// App icon: hh.ru-red squircle, white wordmark, apply arrow underneath.
func drawApp(_ px: Int) -> CGContext {
    let ctx = makeContext(px)

    let body = CGPath(
        roundedRect: CGRect(x: 1, y: 1, width: CANVAS - 2, height: CANVAS - 2),
        cornerWidth: 10.2, cornerHeight: 10.2, transform: nil)
    ctx.saveGState()
    ctx.addPath(body)
    ctx.clip()
    let gradient = CGGradient(
        colorsSpace: CGColorSpaceCreateDeviceRGB(),
        colors: [
            CGColor(red: 1.00, green: 0.23, blue: 0.31, alpha: 1),
            CGColor(red: 0.84, green: 0.00, blue: 0.11, alpha: 1),
        ] as CFArray,
        locations: [0, 1])!
    ctx.drawLinearGradient(
        gradient, start: CGPoint(x: 0, y: 0), end: CGPoint(x: 0, y: CANVAS), options: [])
    ctx.restoreGState()

    ctx.setStrokeColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
    ctx.setLineWidth(4.4)
    addH(ctx, x: 9.8, top: 7.5, bottom: 23.5, radius: 4.8)
    addH(ctx, x: 24.8, top: 7.5, bottom: 23.5, radius: 4.8)
    ctx.strokePath()

    ctx.setStrokeColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.93))
    ctx.setLineWidth(3.6)
    addArrow(ctx, x1: 11.5, x2: 32.5, y: 33.5, head: 4.9)
    ctx.strokePath()

    return ctx
}

/// Guard against the exact bug this file exists to avoid: a corner pixel must
/// be fully transparent, never an opaque white page.
func assertTransparentCorner(_ url: URL) {
    guard let src = CGImageSourceCreateWithURL(url as CFURL, nil),
          let img = CGImageSourceCreateImageAtIndex(src, 0, nil)
    else { fatalError("cannot read back \(url.lastPathComponent)") }
    let ctx = CGContext(
        data: nil, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.interpolationQuality = .none
    // Offset the image so its TOP-LEFT pixel is the one landing in the 1×1
    // viewport. Drawing it into the unit rect instead would average the whole
    // image and report the glyph coverage, not the background.
    let w = CGFloat(img.width), h = CGFloat(img.height)
    ctx.draw(img, in: CGRect(x: 0, y: 1 - h, width: w, height: h))
    let alpha = ctx.data!.load(fromByteOffset: 3, as: UInt8.self)
    if alpha != 0 {
        fatalError("\(url.lastPathComponent): corner alpha \(alpha), expected 0 — background is opaque")
    }
}

// ---- main ----

let outDir = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : ".")
try? FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)

func emit(_ name: String, _ px: Int, _ draw: (Int) -> CGContext, verifyCorner: Bool = false) {
    let url = outDir.appendingPathComponent(name)
    writePNG(draw(px), to: url)
    if verifyCorner { assertTransparentCorner(url) }
    print("  \(name) — \(px)×\(px)")
}

print("tray (template):")
emit("tray.png", 44, drawTray, verifyCorner: true)
emit("tray@2x.png", 88, drawTray, verifyCorner: true)

print("app icon:")
emit("32x32.png", 32, drawApp, verifyCorner: true)
emit("128x128.png", 128, drawApp, verifyCorner: true)
emit("128x128@2x.png", 256, drawApp, verifyCorner: true)
emit("icon.png", 1024, drawApp, verifyCorner: true)

// .iconset for iconutil
let iconset = outDir.appendingPathComponent("icon.iconset")
try? FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for (name, px) in [
    ("icon_16x16.png", 16), ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32), ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128), ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256), ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512), ("icon_512x512@2x.png", 1024),
] {
    writePNG(drawApp(px), to: iconset.appendingPathComponent(name))
}
print("  icon.iconset — 10 sizes (run iconutil -c icns)")
