// Renders an HTML file to an opaque PNG of an exact pixel size with WebKit
// (no dependencies beyond macOS). The page is laid out at `width`×`height`
// CSS pixels; the snapshot is drawn into an RGB bitmap without an alpha
// channel, so the stores accept it as is.
//
//   swift render.swift <page.html> <width> <height> <out.png>

import AppKit
import WebKit

final class Renderer: NSObject, WKNavigationDelegate {
	let width: Int
	let height: Int
	let output: URL
	let view: WKWebView
	let window: NSWindow

	init(page: URL, width: Int, height: Int, output: URL) {
		self.width = width
		self.height = height
		self.output = output
		let frame = NSRect(x: 0, y: 0, width: width, height: height)
		view = WKWebView(frame: frame, configuration: WKWebViewConfiguration())
		window = NSWindow(contentRect: frame, styleMask: [.borderless], backing: .buffered, defer: false)
		super.init()
		window.contentView = view
		window.setFrameOrigin(NSPoint(x: -100_000, y: -100_000))
		view.navigationDelegate = self
		view.loadFileURL(page, allowingReadAccessTo: page.deletingLastPathComponent().deletingLastPathComponent())
	}

	func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
		// Let web fonts and images settle before the snapshot.
		webView.evaluateJavaScript("document.fonts.ready.then(() => true)") { _, _ in
			DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { self.snapshot() }
		}
	}

	func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
		fail("load failed: \(error)")
	}

	func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
		fail("load failed: \(error)")
	}

	func snapshot() {
		let config = WKSnapshotConfiguration()
		config.rect = NSRect(x: 0, y: 0, width: width, height: height)
		view.takeSnapshot(with: config) { image, error in
			guard let image, let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
				self.fail("snapshot failed: \(String(describing: error))")
				return
			}
			let space = CGColorSpace(name: CGColorSpace.sRGB)!
			let (width, height) = (self.width, self.height)
			guard let ctx = CGContext(
				data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
				space: space, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)
			else { return self.fail("no bitmap context") }
			ctx.interpolationQuality = .high
			ctx.draw(cg, in: CGRect(x: 0, y: 0, width: width, height: height))
			let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
			guard let png = rep.representation(using: .png, properties: [:]) else { return self.fail("png encode failed") }
			do {
				try png.write(to: self.output)
			} catch {
				return self.fail("write failed: \(error)")
			}
			exit(0)
		}
	}

	func fail(_ message: String) {
		FileHandle.standardError.write("\(message)\n".data(using: .utf8)!)
		exit(1)
	}
}

let args = CommandLine.arguments
guard args.count == 5, let width = Int(args[2]), let height = Int(args[3]) else {
	FileHandle.standardError.write("usage: swift render.swift <page.html> <width> <height> <out.png>\n".data(using: .utf8)!)
	exit(2)
}

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
let renderer = Renderer(
	page: URL(fileURLWithPath: args[1]).standardizedFileURL,
	width: width, height: height,
	output: URL(fileURLWithPath: args[4]))
DispatchQueue.main.asyncAfter(deadline: .now() + 30) { renderer.fail("timed out") }
app.run()
