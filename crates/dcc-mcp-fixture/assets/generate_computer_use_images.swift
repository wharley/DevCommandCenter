import AppKit

let labels = ["80427159", "39261508", "61743082", "95820374"]
let outputDirectory = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)

for (index, label) in labels.enumerated() {
	let bitmap = NSBitmapImageRep(
		bitmapDataPlanes: nil,
		pixelsWide: 800,
		pixelsHigh: 220,
		bitsPerSample: 8,
		samplesPerPixel: 4,
		hasAlpha: true,
		isPlanar: false,
		colorSpaceName: .deviceRGB,
		bytesPerRow: 0,
		bitsPerPixel: 0
	)!

	NSGraphicsContext.saveGraphicsState()
	NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
	let bounds = NSRect(x: 0, y: 0, width: 800, height: 220)
	NSColor.white.setFill()
	bounds.fill()

	let caption = NSString(string: "DCC COMPUTER USE IMAGE CHECK")
	let captionAttributes: [NSAttributedString.Key: Any] = [
		.font: NSFont.systemFont(ofSize: 22, weight: .regular),
		.foregroundColor: NSColor(calibratedRed: 0.25, green: 0.31, blue: 0.39, alpha: 1),
	]
	caption.draw(at: NSPoint(x: 42, y: 158), withAttributes: captionAttributes)

	let code = NSString(string: label)
	let codeAttributes: [NSAttributedString.Key: Any] = [
		.font: NSFont.monospacedSystemFont(ofSize: 78, weight: .semibold),
		.foregroundColor: NSColor(calibratedRed: 0.07, green: 0.1, blue: 0.16, alpha: 1),
	]
	let codeSize = code.size(withAttributes: codeAttributes)
	code.draw(
		at: NSPoint(x: (800 - codeSize.width) / 2, y: 62),
		withAttributes: codeAttributes
	)
	NSGraphicsContext.restoreGraphicsState()

	let data = bitmap.representation(using: .png, properties: [:])!
	let destination = outputDirectory.appendingPathComponent("image-check-\(index).png")
	try data.write(to: destination)
}
