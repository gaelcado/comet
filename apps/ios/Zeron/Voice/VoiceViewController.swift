import UIKit

/// Settings and foreground call controls share the app-owned voice session.
final class VoiceViewController: UITableViewController {
    private let app: AppModel
    private var token: UUID?
    private var appToken: AnyObject?
    private let orb = VoiceOrbView()
    private let status = UILabel()
    private let caption = UILabel()
    private var devices: [DeviceView] = []
    private let styles: [String?] = [nil, "juniper", "maple", "spruce", "ember", "vale", "breeze", "arbor", "sol", "cove"]
    init(app: AppModel) { self.app = app; super.init(style: .insetGrouped); title = "Voice" }
    required init?(coder: NSCoder) { fatalError() }
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = Palette.background
        let header = UIView(frame: CGRect(x: 0, y: 0, width: 320, height: 270))
        orb.translatesAutoresizingMaskIntoConstraints = false
        status.translatesAutoresizingMaskIntoConstraints = false
        caption.translatesAutoresizingMaskIntoConstraints = false
        status.font = Fonts.ui(.sansMedium, 16); status.textColor = Palette.text; status.textAlignment = .center; status.numberOfLines = 2
        caption.font = Fonts.ui(.sans, 14); caption.textColor = Palette.secondary; caption.numberOfLines = 3; caption.textAlignment = .center
        for child in [orb, status, caption] { header.addSubview(child) }
        NSLayoutConstraint.activate([
            orb.centerXAnchor.constraint(equalTo: header.centerXAnchor), orb.topAnchor.constraint(equalTo: header.topAnchor, constant: 12),
            orb.widthAnchor.constraint(equalToConstant: 140), orb.heightAnchor.constraint(equalToConstant: 140),
            status.topAnchor.constraint(equalTo: orb.bottomAnchor, constant: 12), status.leadingAnchor.constraint(equalTo: header.leadingAnchor, constant: 20), status.trailingAnchor.constraint(equalTo: header.trailingAnchor, constant: -20),
            caption.topAnchor.constraint(equalTo: status.bottomAnchor, constant: 8), caption.leadingAnchor.constraint(equalTo: status.leadingAnchor), caption.trailingAnchor.constraint(equalTo: status.trailingAnchor),
        ])
        tableView.tableHeaderView = header
        token = app.voice.observe { [weak self] in self?.refresh() }
        appToken = app.observe { [weak self] in self?.refresh() }
        refresh()
    }
    private func refresh() {
        devices = app.client?.devices() ?? []
        let voice = app.voice
        status.text = voice.reason ?? (voice.live ? "\(voice.phase.capitalized) · \(voice.hostName)" : "Codex voice")
        caption.text = voice.caption
        orb.level = voice.level
        // Level frames do not need to rebuild table cells.
        let signature = "\(voice.live)|\(voice.muted)|\(voice.selectedHost ?? "")|\(voice.selectedStyle ?? "")|" + devices.map { "\($0.id):\($0.online):\($0.capabilities)" }.joined()
        if signature != lastSignature { lastSignature = signature; tableView.reloadData() }
    }
    private var lastSignature = ""
    override func numberOfSections(in tableView: UITableView) -> Int { 3 }
    override func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
        switch section { case 0: max(1, devices.count); case 1: 1; default: app.voice.live ? 2 : 1 }
    }
    override func tableView(_ tableView: UITableView, titleForHeaderInSection section: Int) -> String? {
        switch section { case 0: "Codex voice device"; case 1: "Codex voice style"; default: nil }
    }
    override func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = UITableViewCell(style: .subtitle, reuseIdentifier: nil)
        cell.backgroundColor = Palette.elevated; cell.textLabel?.textColor = Palette.text; cell.detailTextLabel?.textColor = Palette.secondary
        if indexPath.section == 0 {
            guard !devices.isEmpty else { cell.textLabel?.text = "No registered devices"; cell.selectionStyle = .none; return cell }
            let device = devices[indexPath.row]
            let compatible = device.isExecutionHost && device.capabilities.contains("voice-client-media-v1")
            cell.textLabel?.text = device.name
            cell.detailTextLabel?.text = !device.isExecutionHost ? "Cannot host Codex" : !device.online ? "Offline" : !compatible ? "Update Zeron / enable remote voice" : app.voice.live ? "End the call to change devices" : "Available"
            cell.accessoryType = device.id == app.voice.selectedHost ? .checkmark : .none
            cell.isUserInteractionEnabled = device.online && compatible && !app.voice.live
            cell.textLabel?.textColor = cell.isUserInteractionEnabled ? Palette.text : Palette.secondary
        } else if indexPath.section == 1 {
            cell.textLabel?.text = app.voice.selectedStyle?.capitalized ?? "Codex default"
            cell.detailTextLabel?.text = "Applies to the next call"; cell.accessoryType = .disclosureIndicator
        } else {
            cell.textLabel?.text = !app.voice.live ? "Start voice" : indexPath.row == 0 ? (app.voice.muted ? "Unmute" : "Mute") : "End call"
            cell.textLabel?.textColor = app.voice.live && indexPath.row == 1 ? Palette.danger : Palette.accent
            cell.accessibilityIdentifier = !app.voice.live ? "voice-start" : indexPath.row == 0 ? "voice-mute" : "voice-end"
        }
        return cell
    }
    override func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath, animated: true)
        switch indexPath.section {
        case 0: if devices.indices.contains(indexPath.row), !app.voice.live { app.voice.selectedHost = devices[indexPath.row].id }
        case 1:
            let sheet = UIAlertController(title: "Codex voice style", message: "Applies to the next call", preferredStyle: .actionSheet)
            for style in styles { sheet.addAction(UIAlertAction(title: style?.capitalized ?? "Codex default", style: .default) { [weak self] _ in self?.app.voice.selectedStyle = style }) }
            sheet.addAction(UIAlertAction(title: "Cancel", style: .cancel))
            sheet.popoverPresentationController?.sourceView = tableView.cellForRow(at: indexPath)
            present(sheet, animated: true)
        default:
            if !app.voice.live { app.voice.start() } else if indexPath.row == 0 { app.voice.toggleMute() } else { app.voice.stop() }
        }
    }
    deinit {
        if let token { let app = app; Task { @MainActor in app.voice.removeObserver(token) } }
    }
}

private final class VoiceOrbView: UIView {
    var level: CGFloat = 0 { didSet { animate() } }
    private let gradient = CAGradientLayer()
    override init(frame: CGRect) {
        super.init(frame: frame)
        gradient.type = .radial
        gradient.colors = [UIColor.systemCyan.cgColor, UIColor.systemIndigo.cgColor, UIColor.systemPurple.cgColor]
        gradient.startPoint = CGPoint(x: 0.35, y: 0.3); gradient.endPoint = CGPoint(x: 1, y: 1)
        layer.addSublayer(gradient)
        isAccessibilityElement = true; accessibilityLabel = "Voice activity"
    }
    required init?(coder: NSCoder) { fatalError() }
    override func layoutSubviews() { super.layoutSubviews(); gradient.frame = bounds; gradient.cornerRadius = bounds.width / 2; gradient.masksToBounds = true }
    private func animate() {
        let scale = 0.92 + min(1, max(0, level - 0.015) * 3) * 0.08
        UIView.animate(withDuration: scale > transform.a ? 0.08 : 0.3, delay: 0, options: [.beginFromCurrentState, .allowUserInteraction]) { self.transform = CGAffineTransform(scaleX: scale, y: scale) }
    }
}
