import UIKit

/// Voice settings: which registered device runs Codex for a call, and its
/// voice. Calls themselves start from the bottom bar.
final class VoiceViewController: UITableViewController {
    private let app: AppModel
    private var token: AnyObject?
    private var appToken: AnyObject?
    private var devices: [DeviceView] = []
    private var lastSignature = ""

    init(app: AppModel) {
        self.app = app
        super.init(style: .insetGrouped)
        title = "Voice"
    }

    required init?(coder: NSCoder) { fatalError() }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = Palette.background
        token = app.voice.observe { [weak self] in self?.refresh() }
        appToken = app.observe { [weak self] in self?.refresh() }
        refresh()
    }

    private func refresh() {
        // Phones can't host Codex; list only the machines that can.
        devices = (app.client?.devices() ?? []).filter(\.isExecutionHost)
        let voice = app.voice
        // Level frames do not need to rebuild table cells.
        let signature = "\(voice.live)|\(voice.selectedHost ?? "")|\(voice.selectedStyle ?? "")|\(voice.styles)|" + devices.map { "\($0.id):\($0.online):\($0.capabilities)" }.joined()
        if signature != lastSignature { lastSignature = signature; tableView.reloadData() }
    }

    override func numberOfSections(in tableView: UITableView) -> Int { 2 }

    override func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
        section == 0 ? max(1, devices.count) : 1
    }

    override func tableView(_ tableView: UITableView, titleForHeaderInSection section: Int) -> String? {
        section == 0 ? "Codex runs on" : "Voice"
    }

    override func tableView(_ tableView: UITableView, titleForFooterInSection section: Int) -> String? {
        section == 0
            ? "Audio stays on this iPhone; Codex, its sign-in and its tools run on this device. Start a call from the waveform in the bottom bar."
            : "Hold the phone to your ear during a call to use the earpiece, like a phone call."
    }

    override func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = UITableViewCell(style: .subtitle, reuseIdentifier: nil)
        cell.backgroundColor = Palette.elevated
        cell.textLabel?.textColor = Palette.text
        cell.detailTextLabel?.textColor = Palette.secondary
        let voice = app.voice
        if indexPath.section == 0 {
            guard !devices.isEmpty else {
                cell.textLabel?.text = "No Mac or server registered"
                cell.selectionStyle = .none
                return cell
            }
            let device = devices[indexPath.row]
            let compatible = RemoteVoiceController.compatible(device)
            cell.textLabel?.text = device.name
            cell.detailTextLabel?.text = !device.online ? "Offline" : !compatible ? "Update Zeron and enable remote voice" : voice.live ? "End the call to change devices" : "Available"
            cell.accessoryType = device.id == voice.selectedHost ? .checkmark : .none
            cell.imageView?.image = UIImage(systemName: "desktopcomputer")
            cell.imageView?.tintColor = Palette.secondary
            cell.isUserInteractionEnabled = device.online && compatible && !voice.live
            cell.textLabel?.textColor = cell.isUserInteractionEnabled ? Palette.text : Palette.secondary
        } else {
            cell.textLabel?.text = voice.selectedStyle?.capitalized ?? "Codex default"
            cell.detailTextLabel?.text = "Applies to the next call"
            cell.accessoryType = .disclosureIndicator
            cell.accessibilityIdentifier = "voice-style"
        }
        return cell
    }

    override func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath, animated: true)
        let voice = app.voice
        if indexPath.section == 0 {
            if devices.indices.contains(indexPath.row), !voice.live { voice.selectedHost = devices[indexPath.row].id }
            return
        }
        let sheet = UIAlertController(title: "Voice", message: "Applies to the next call", preferredStyle: .actionSheet)
        for style in [nil] + voice.styles.map(Optional.some) {
            sheet.addAction(UIAlertAction(title: style?.capitalized ?? "Codex default", style: .default) { _ in voice.selectedStyle = style })
        }
        sheet.addAction(UIAlertAction(title: "Cancel", style: .cancel))
        sheet.popoverPresentationController?.sourceView = tableView.cellForRow(at: indexPath)
        present(sheet, animated: true)
    }
}
