import XCTest
@testable import Zeron

final class RemoteVoiceLifecycleTests: XCTestCase {
    func testCancelledContinuationIgnoresLateNativeSuccess() async {
        let slot = VoiceContinuation<String>()
        slot.finish(.failure(CancellationError()))
        do {
            let _: String = try await withCheckedThrowingContinuation { continuation in
                slot.install(continuation)
                slot.finish(.success("late offer"))
            }
            XCTFail("Cancelled negotiation must not produce an offer")
        } catch { XCTAssertTrue(error is CancellationError) }
    }

    func testNativeCompletionResumesOnlyOnce() async throws {
        let slot = VoiceContinuation<Int>()
        let value = try await withCheckedThrowingContinuation { continuation in
            slot.install(continuation)
            slot.finish(.success(1))
            slot.finish(.success(2))
            slot.finish(.failure(CancellationError()))
        }
        XCTAssertEqual(value, 1)
    }

    @MainActor
    func testClosedEndpointCannotReactivateOrProduceOffer() async {
        let peer = CodexVoicePeer()
        peer.close()
        peer.close()
        XCTAssertThrowsError(try peer.setMuted(false))
        do { _ = try await peer.offer(); XCTFail("Closed peer cannot negotiate") } catch {}
        do { try await peer.apply(answer: "late answer"); XCTFail("Closed peer cannot apply answer") } catch {}
    }
}
