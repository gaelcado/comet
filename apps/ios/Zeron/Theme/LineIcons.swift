import SwiftUI

/// Compatibility names for existing checkout controls, backed by the custom family.
enum LineIcon: String {
    case gitBranch = "branch"
    case pullRequest = "pull-request"
    case merge
    case folder
    case folderWithFiles = "repository"
}
struct LineIconView: View {
    let icon: LineIcon
    var size: CGFloat
    var color: Color
    init(_ icon: LineIcon, size: CGFloat = 14, color: Color = Theme.textMuted) {
        self.icon = icon; self.size = size; self.color = color
    }
    var body: some View {
        ZeronIcon(icon.rawValue, size: size).foregroundStyle(color)
    }
}
