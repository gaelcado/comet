import SwiftUI
import XCTest
@testable import Zeron

@MainActor
final class CustomIconTests: XCTestCase {
    func testEntireFamilyAndEveryMotionFrameDecode() {
        let library = CustomIconLibrary.shared
        XCTAssertEqual(library.icons.count, 135)
        XCTAssertEqual(library.motions.count, 36)
        for icon in library.icons.values {
            for contour in icon.paths + icon.smallPaths {
                XCTAssertFalse(SVGPathParser.path(from: contour.d).isEmpty, icon.name)
            }
        }
        for motion in library.motions {
            XCTAssertNotNil(library.icons[motion.from])
            XCTAssertNotNil(library.icons[motion.to])
            XCTAssertEqual(motion.frames.count, 97)
            XCTAssertEqual(motion.smallFrames.count, 97)
            for frame in motion.frames + motion.smallFrames {
                XCTAssertFalse(frame.isEmpty)
                for contour in frame {
                    XCTAssertFalse(SVGPathParser.path(from: contour.d).isEmpty)
                }
            }
        }
    }
    func testAliasesAndUIKitMenuImagesUseTheCustomFamily() {
        let library = CustomIconLibrary.shared
        for (alias, name) in library.aliases {
            XCTAssertNotNil(library.icons[name], alias)
            let image = library.menuImage(for: alias)
            XCTAssertEqual(image.renderingMode, .alwaysTemplate)
            XCTAssertEqual(image.size, CGSize(width: 18, height: 18))
            XCTAssertNotNil(image.cgImage)
        }
    }
    func testCanvasRendersBothOpticalSizes() {
        for name in CustomIconLibrary.shared.icons.keys {
        for size: CGFloat in [12, 16, 24] {
            let renderer = ImageRenderer(content: ZeronIcon(name, size: size).foregroundStyle(.black))
            let image = renderer.uiImage
            XCTAssertNotNil(image)
            XCTAssertEqual(image?.size, CGSize(width: size, height: size))
        }
        }
    }
}
