// README capture helper. Displays an unmodified, bundled macOS wallpaper in
// an independent window BEHIND VibeShell, so AppKit performs the real blur.
// The image never becomes a CSS/VibeShell background or product asset.
import AppKit
import QuartzCore

guard CommandLine.arguments.count == 3,
      let targetPID = Int32(CommandLine.arguments[1]),
      let image = NSImage(contentsOfFile: CommandLine.arguments[2]),
      let bitmap = image.cgImage(forProposedRect: nil, context: nil, hints: nil)
else {
    fputs("Usage: glass-backdrop <vibeshell-PID> <macOS-wallpaper-path>\n", stderr)
    exit(2)
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
guard let virtualScreen = NSScreen.screens.first(where: {
    $0.localizedName == "VibeShell README Capture"
}) else {
    fputs("Screenshot backdrop requires the isolated virtual display.\n", stderr)
    exit(3)
}
let window = NSWindow(
    contentRect: virtualScreen.frame,
    styleMask: [.borderless],
    backing: .buffered,
    defer: false
)
window.isReleasedWhenClosed = false
window.ignoresMouseEvents = true
window.hasShadow = false
window.isOpaque = true
window.backgroundColor = .black
// Keep the wallpaper above unrelated normal apps. The isolated capture build
// raises VibeShell itself to this level after the wallpaper is ready.
// Neither this material nor a simulated gradient is drawn inside VibeShell.
window.level = .normal
let content = NSView(frame: window.contentView!.bounds)
content.autoresizingMask = [.width, .height]
content.wantsLayer = true
content.layer?.contents = bitmap
content.layer?.contentsGravity = .resizeAspectFill
content.layer?.masksToBounds = true
window.contentView = content
window.orderFrontRegardless()
NSRunningApplication(processIdentifier: targetPID)?.activate(options: [.activateAllWindows])
print("README wallpaper window behind native VibeShell, pid=\(targetPID)")
fflush(stdout)
Timer.scheduledTimer(withTimeInterval: 600, repeats: false) { _ in app.terminate(nil) }
app.run()
