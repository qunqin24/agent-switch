#!/usr/bin/env swift

import AppKit
import Foundation

guard CommandLine.arguments.count == 3 else {
    fputs("Usage: generate-macos-app-icon.swift <transparent-logo.png> <output.png>\n", stderr)
    exit(2)
}

let inputURL = URL(fileURLWithPath: CommandLine.arguments[1])
let outputURL = URL(fileURLWithPath: CommandLine.arguments[2])
let canvasPixels = 1024

guard let logo = NSImage(contentsOf: inputURL) else {
    fputs("Unable to read logo: \(inputURL.path)\n", stderr)
    exit(1)
}

guard let bitmap = NSBitmapImageRep(
    bitmapDataPlanes: nil,
    pixelsWide: canvasPixels,
    pixelsHigh: canvasPixels,
    bitsPerSample: 8,
    samplesPerPixel: 4,
    hasAlpha: true,
    isPlanar: false,
    colorSpaceName: .deviceRGB,
    bytesPerRow: 0,
    bitsPerPixel: 0
) else {
    fputs("Unable to create the icon bitmap\n", stderr)
    exit(1)
}

guard let graphicsContext = NSGraphicsContext(bitmapImageRep: bitmap) else {
    fputs("Unable to create the icon graphics context\n", stderr)
    exit(1)
}

NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = graphicsContext

let context = graphicsContext.cgContext
context.clear(CGRect(x: 0, y: 0, width: canvasPixels, height: canvasPixels))
context.setShouldAntialias(true)
context.interpolationQuality = .high

// A centered continuous-corner silhouette keeps the white plate inside the
// macOS app-icon grid while leaving transparent outer corners for the system.
let plateInset: CGFloat = 82
let plateSide = CGFloat(canvasPixels) - plateInset * 2
let plateCenter = CGFloat(canvasPixels) / 2
let radius = plateSide / 2
let exponent: CGFloat = 5
let path = CGMutablePath()
let pointCount = 720

for index in 0...pointCount {
    let angle = CGFloat(index) / CGFloat(pointCount) * 2 * .pi
    let cosine = cos(angle)
    let sine = sin(angle)
    let x = plateCenter
        + radius * copysign(pow(abs(cosine), 2 / exponent), cosine)
    let y = plateCenter
        + radius * copysign(pow(abs(sine), 2 / exponent), sine)
    if index == 0 {
        path.move(to: CGPoint(x: x, y: y))
    } else {
        path.addLine(to: CGPoint(x: x, y: y))
    }
}
path.closeSubpath()

context.addPath(path)
context.clip()

let systemLightColors = [
    NSColor(srgbRed: 1, green: 1, blue: 1, alpha: 1).cgColor,
    NSColor(srgbRed: 0.94, green: 0.95, blue: 0.96, alpha: 1).cgColor,
] as CFArray
guard let systemLightGradient = CGGradient(
    colorsSpace: CGColorSpaceCreateDeviceRGB(),
    colors: systemLightColors,
    locations: [0, 1]
) else {
    fputs("Unable to create the System Light background gradient\n", stderr)
    exit(1)
}
context.drawLinearGradient(
    systemLightGradient,
    start: CGPoint(x: plateInset, y: CGFloat(canvasPixels) - plateInset),
    end: CGPoint(x: CGFloat(canvasPixels) - plateInset, y: plateInset),
    options: []
)
context.resetClip()

// Draw the original artwork without altering its geometry or gradient.
let logoSide: CGFloat = 820
let logoRect = NSRect(
    x: (CGFloat(canvasPixels) - logoSide) / 2,
    y: (CGFloat(canvasPixels) - logoSide) / 2,
    width: logoSide,
    height: logoSide
)
// The legacy PNG contains a one-pixel export frame at the canvas edge. Crop a
// further pixel to prevent that frame from becoming visible after scaling.
let sourceRect = NSRect(
    x: 2,
    y: 2,
    width: logo.size.width - 4,
    height: logo.size.height - 4
)
logo.draw(in: logoRect, from: sourceRect, operation: .sourceOver, fraction: 1)

graphicsContext.flushGraphics()
NSGraphicsContext.restoreGraphicsState()

guard let png = bitmap.representation(using: .png, properties: [:]) else {
    fputs("Unable to encode the icon PNG\n", stderr)
    exit(1)
}

do {
    try png.write(to: outputURL, options: .atomic)
} catch {
    fputs("Unable to write \(outputURL.path): \(error)\n", stderr)
    exit(1)
}
