import XCTest

final class AgentUpdatesTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }
    private func launch(_ args: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-route", "more"] + args
        app.launch()
        XCTAssertTrue(app.cells["device:dev-mac"].waitForExistence(timeout: 15))
        return app
    }
    private func capture(_ app: XCUIApplication, _ name: String) {
        let image = XCTAttachment(screenshot: app.screenshot())
        image.name = name
        image.lifetime = .keepAlways
        add(image)
    }
    func testUpdatesCancelPolicyAndReopen() {
        let app = launch(["-appearance", "1"])
        capture(app, "devices")
        app.cells["device:dev-mac"].tap()
        let update = app.buttons["update-action-claude-code"]
        XCTAssertTrue(update.waitForExistence(timeout: 10))
        capture(app, "updates-available")
        update.tap()
        let cancel = app.buttons.matching(NSPredicate(format: "identifier == 'update-action-claude-code' AND label == 'Cancel Claude Code'")).firstMatch
        XCTAssertTrue(cancel.waitForExistence(timeout: 4))
        capture(app, "updates-waiting")
        cancel.tap()
        XCTAssertTrue(app.buttons["Update Claude Code"].waitForExistence(timeout: 5))
        app.buttons["update-policy-claude-code"].tap()
        app.buttons["Off"].tap()
        XCTAssertTrue(app.staticTexts["Monitoring is off"].waitForExistence(timeout: 5))
        app.buttons["update-policy-claude-code"].tap()
        app.buttons["Notify"].tap()
        XCTAssertTrue(app.buttons["Update Claude Code"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.cells["device:dev-mac"].tap()
        XCTAssertTrue(app.buttons["Update Claude Code"].waitForExistence(timeout: 5))
        app.swipeUp()
        capture(app, "updates-details")
        let retry = app.buttons["update-action-opencode"]
        XCTAssertTrue(retry.waitForExistence(timeout: 5))
        retry.tap()
        XCTAssertTrue(app.buttons["Cancel OpenCode"].waitForExistence(timeout: 5))
    }
    func testLargeTextLayoutAndAccessibilityLabels() {
        let app = launch(["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL", "-appearance", "2"])
        app.cells["device:dev-mac"].tap()
        let action = app.buttons["update-action-claude-code"]
        XCTAssertTrue(action.waitForExistence(timeout: 10))
        if !action.isHittable { app.swipeUp() }
        XCTAssertTrue(action.isHittable)
        XCTAssertEqual(action.label, "Update Claude Code")
        XCTAssertGreaterThanOrEqual(action.frame.height, 44)
        XCTAssertGreaterThanOrEqual(action.frame.minX, 0)
        XCTAssertLessThanOrEqual(action.frame.maxX, app.windows.firstMatch.frame.maxX)
        XCTAssertTrue(app.buttons["update-policy-claude-code"].exists)
        capture(app, "updates-accessibility-text-dark")
    }
    func testOfflineAndUnsupportedDevices() {
        let app = launch()
        app.cells["device:dev-offline"].tap()
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS 'Offline. Connect'")).firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["updates-check"].isEnabled)
        capture(app, "updates-offline")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.cells["device:dev-studio"].tap()
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS 'Update Zeron on this device'")).firstMatch.waitForExistence(timeout: 5))
        capture(app, "updates-unsupported")
    }
}
