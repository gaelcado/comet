import UIKit

@MainActor
final class RemoteVoiceController {
    static var enabled: Bool {
        ProcessInfo.processInfo.arguments.contains("-remote-voice") || ProcessInfo.processInfo.environment["ZERON_REMOTE_VOICE"] == "1"
    }
    private weak var app: AppModel?
    private var call: VoiceCall?
    private var media: NativeVoiceMedia?
    private var generation: UInt64 = 0
    private var finals: Set<String> = []
    private var partialItem: String?
    private var observers: [UUID: () -> Void] = [:]
    private(set) var phase = "closed"
    private(set) var caption = ""
    private(set) var reason: String?
    private(set) var muted = false
    private(set) var level: CGFloat = 0
    private(set) var hostName = ""
    var live: Bool { call != nil }
    init(app: AppModel) { self.app = app }
    private var preferenceKey: String {
        "remoteVoice.\(app?.client?.userId() ?? "none").\(app?.client?.orgId() ?? "none")"
    }
    var selectedHost: String? {
        get { UserDefaults.standard.string(forKey: preferenceKey + ".host") }
        set { if !live { UserDefaults.standard.set(newValue, forKey: preferenceKey + ".host"); changed() } }
    }
    var selectedStyle: String? {
        get { UserDefaults.standard.string(forKey: preferenceKey + ".style") }
        set { UserDefaults.standard.set(newValue, forKey: preferenceKey + ".style"); changed() }
    }
    func observe(_ callback: @escaping () -> Void) -> UUID { let id = UUID(); observers[id] = callback; return id }
    func removeObserver(_ id: UUID) { observers[id] = nil }
    private func changed() { for callback in observers.values { callback() } }

    func start() {
        guard Self.enabled, !live, let client = app?.client, let host = selectedHost,
              let device = client.devices().first(where: { $0.id == host }),
              device.online, device.isExecutionHost, device.capabilities.contains("voice-client-media-v1") else {
            reason = "Choose an available Codex voice device."; changed(); return
        }
        generation &+= 1
        finals.removeAll(); partialItem = nil; caption = ""; reason = nil; muted = false; level = 0
        hostName = device.name; phase = "starting"
        let media = NativeVoiceMedia()
        let listener = VoiceListenerBridge(controller: self, generation: generation)
        self.media = media
        media.onFailure = { [weak self] in self?.stop(reason: "The audio connection closed. Start a new call to reconnect.") }
        let call = client.startVoice(hostDeviceId: host, voice: selectedStyle, media: media, listener: listener)
        self.call = call
        media.call = call
        changed()
    }
    func toggleMute() {
        guard live else { return }
        muted.toggle()
        media?.muteLocally(muted)
        call?.setMuted(muted: muted)
        changed()
    }
    func stop(reason: String? = nil) {
        generation &+= 1
        media?.close() // Local capture stops before remote control.
        call?.stop()
        call = nil; media = nil; phase = reason == nil ? "closed" : "failed"
        self.reason = reason; level = 0
        changed()
    }
    fileprivate func receive(_ json: String, generation: UInt64) {
        guard generation == self.generation, live,
              let data = json.data(using: .utf8),
              let event = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return }
        switch event["type"] as? String {
        case "snapshot":
            if let snapshot = event["snapshot"] as? [String: Any] { phase = snapshot["phase"] as? String ?? phase }
        case "partial":
            let item = event["item_id"] as? String ?? ""
            guard !finals.contains(item) else { return }
            if partialItem != item || partialItem == nil { caption = "" }
            partialItem = item
            caption = String((caption + (event["text"] as? String ?? "")).suffix(32_768))
        case "final":
            if let transcript = event["transcript"] as? [String: Any], let item = transcript["itemId"] as? String,
               finals.count < 4096, finals.insert(item).inserted { partialItem = nil; caption = transcript["text"] as? String ?? "" }
        case "levels":
            let microphone = muted ? 0 : (event["microphone"] as? Double ?? 0)
            level = CGFloat(max(microphone, event["speaker"] as? Double ?? 0) / 65535)
        default: break
        }
        changed()
    }
    fileprivate func closed(_ reason: String?, generation: UInt64) {
        guard generation == self.generation else { return }
        stop(reason: reason.map { code in
            switch code {
            case "ChatgptRequired": "Sign in to Codex with ChatGPT on the selected device."
            case "Busy": "The selected device already has a voice call."
            case "Disabled", "Unsupported": "Update Zeron and enable remote voice on the selected device."
            case "RemoteHost": "The selected device is unavailable."
            default: "Voice ended (\(code)). Check microphone permission and the selected device."
            }
        })
    }
}

private final class VoiceListenerBridge: VoiceSessionListener, @unchecked Sendable {
    private weak var controller: RemoteVoiceController?
    private let generation: UInt64
    init(controller: RemoteVoiceController, generation: UInt64) { self.controller = controller; self.generation = generation }
    func onVoiceEvent(eventJson: String) {
        let controller = controller; let generation = generation
        Task { @MainActor [weak controller] in controller?.receive(eventJson, generation: generation) }
    }
    func onVoiceClosed(reason: String?) {
        let controller = controller; let generation = generation
        Task { @MainActor [weak controller] in controller?.closed(reason, generation: generation) }
    }
}
