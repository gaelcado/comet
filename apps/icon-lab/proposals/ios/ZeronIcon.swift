import SwiftUI
import UIKit

/// The shared custom family. Source: apps/icon-lab/glyphs.json and motions.json.
struct ZeronIcon: View {
    private let name: String
    private var size: CGFloat = 16
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var previous: String?
    @State private var motion: Int?
    @State private var progress: Double = 0

    init(_ name: String, size: CGFloat = 16) {
        self.name = name
        self.size = size
    }
    init(systemName: String) {
        self.name = CustomIconLibrary.shared.name(for: systemName)
    }
    func iconSize(_ size: CGFloat) -> Self {
        var copy = self
        copy.size = size
        return copy
    }
    var body: some View {
        CustomIconCanvas(name: name, motion: motion, progress: progress, size: size)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
            .task(id: name) {
                let old = previous
                previous = name
                guard !reduceMotion, let old, old != name,
                      let index = CustomIconLibrary.shared.motions.firstIndex(where: {
                          ($0.from == old && $0.to == name) || ($0.to == old && $0.from == name)
                      }) else {
                    var transaction = Transaction()
                    transaction.disablesAnimations = true
                    withTransaction(transaction) { motion = nil }
                    return
                }
                let spec = CustomIconLibrary.shared.motions[index]
                if motion != index {
                    var transaction = Transaction()
                    transaction.disablesAnimations = true
                    withTransaction(transaction) {
                        motion = index
                        progress = old == spec.from ? 0 : 1
                    }
                    await Task.yield()
                }
                guard !Task.isCancelled else { return }
                withAnimation(.spring(response: spec.duration / 1000, dampingFraction: 1)) {
                    progress = name == spec.to ? 1 : 0
                }
            }
            .onChange(of: reduceMotion) { _, reduced in
                if reduced {
                    var transaction = Transaction()
                    transaction.disablesAnimations = true
                    withTransaction(transaction) { motion = nil }
                }
            }
    }
}

private struct CustomIconCanvas: View, Animatable {
    let name: String
    let motion: Int?
    var progress: Double
    let size: CGFloat
    var animatableData: Double {
        get { progress }
        set { progress = newValue }
    }
    var body: some View {
        Canvas { context, bounds in
            let library = CustomIconLibrary.shared
            let small = size <= 16
            let paths: [CustomIconLibrary.Contour]
            if let motion {
                let bank = library.motions[motion]
                paths = (small ? bank.smallFrames : bank.frames)[Int((min(1, max(0, progress)) * 96).rounded())]
            } else {
                guard let glyph = library.icons[name] else { return }
                paths = small ? glyph.smallPaths : glyph.paths
            }
            let scale = min(bounds.width, bounds.height) / 24
            let transform = CGAffineTransform(scaleX: scale, y: scale)
            for contour in paths {
                let path = SVGPathParser.path(from: contour.d).applying(transform)
                var layer = context
                layer.opacity = contour.opacity ?? 1
                if contour.fill > 0 {
                    var fill = layer
                    fill.opacity *= contour.fill
                    fill.fill(path, with: .foreground)
                }
                if contour.stroke != 0 {
                    layer.stroke(path, with: .foreground, style: StrokeStyle(
                        lineWidth: 1.75 * scale, lineCap: .round, lineJoin: .round))
                }
            }
        }
    }
}

struct CustomIconLibrary {
    struct Contour: Decodable {
        let d: String
        let fill: Double
        let stroke: Int?
        let opacity: Double?
    }
    struct Glyph: Decodable {
        let name: String
        let paths: [Contour]
        let smallPaths: [Contour]
    }
    struct Motion: Decodable {
        let from: String
        let to: String
        let duration: Double
        let frames: [[Contour]]
        let smallFrames: [[Contour]]
    }
    private struct Payload: Decodable {
        let icons: [Glyph]
        let aliases: [String: String]
        let motions: [Motion]
    }
    let icons: [String: Glyph]
    let aliases: [String: String]
    let motions: [Motion]
    let motionNames: Set<String>
    static let shared: CustomIconLibrary = {
        guard let url = Bundle.main.url(forResource: "CustomIconLibrary", withExtension: "json"),
              let bytes = try? Data(contentsOf: url),
              let payload = try? JSONDecoder().decode(Payload.self, from: bytes) else {
            preconditionFailure("Missing or invalid generated custom icon library")
        }
        return CustomIconLibrary(icons: Dictionary(uniqueKeysWithValues: payload.icons.map { ($0.name, $0) }),
                                 aliases: payload.aliases, motions: payload.motions,
                                 motionNames: Set(payload.motions.flatMap { [$0.from, $0.to] }))
    }()
    func name(for symbol: String) -> String {
        if let name = aliases[symbol] { return name }
        if icons[symbol] != nil { return symbol }
        assertionFailure("Unmapped icon: \(symbol)")
        return "error"
    }
}

/// UIKit-backed menus need an Image label; render our vector source, never an SF Symbol.
extension CustomIconLibrary {
    func menuImage(for symbol: String) -> UIImage {
        let name = name(for: symbol)
        let contours = icons[name]!.paths
        let format = UIGraphicsImageRendererFormat()
        format.scale = 3
        return UIGraphicsImageRenderer(size: CGSize(width: 18, height: 18), format: format).image { renderer in
            let context = renderer.cgContext
            context.scaleBy(x: 0.75, y: 0.75)
            context.setLineWidth(1.75)
            context.setLineCap(.round)
            context.setLineJoin(.round)
            context.setStrokeColor(UIColor.black.cgColor)
            context.setFillColor(UIColor.black.cgColor)
            for contour in contours {
                let path = SVGPathParser.path(from: contour.d).cgPath
                if contour.fill > 0 {
                    context.setAlpha(contour.fill)
                    context.addPath(path)
                    context.fillPath()
                }
                if contour.stroke != 0 {
                    context.setAlpha(1)
                    context.addPath(path)
                    context.strokePath()
                }
            }
        }.withRenderingMode(.alwaysTemplate)
    }
}
extension Label where Title == Text, Icon == Image {
    init(_ title: String, zeronIcon: String) {
        self.init { Text(title) } icon: {
            Image(uiImage: CustomIconLibrary.shared.menuImage(for: zeronIcon))
        }
    }
}
extension Button where Label == SwiftUI.Label<Text, Image> {
    init(_ title: String, zeronIcon: String, role: ButtonRole? = nil, action: @escaping () -> Void) {
        self.init(role: role, action: action) { SwiftUI.Label(title, zeronIcon: zeronIcon) }
    }
}
