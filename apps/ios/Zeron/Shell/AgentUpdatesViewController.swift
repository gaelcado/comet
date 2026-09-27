import UIKit

/// A viewport onto one host's Rust-owned update state.
final class AgentUpdatesViewController: UIViewController {
    private let app: AppModel
    private let deviceId: String
    private var handle: AgentUpdatesHandle?
    private var observer: AnyObject?
    private var workspaceObserver: AnyObject?
    private var list: UICollectionView!
    private var dataSource: UICollectionViewDiffableDataSource<String, String>!
    private var current: AgentUpdateSnapshot?
    private var expanded = Set<String>()
    private var checkButton: UIBarButtonItem!

    init(app: AppModel, deviceId: String) {
        self.app = app
        self.deviceId = deviceId
        super.init(nibName: nil, bundle: nil)
        title = "Agent Updates"
    }
    required init?(coder: NSCoder) { fatalError() }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = Palette.background
        NotificationCenter.default.addObserver(self, selector: #selector(render), name: UIAccessibility.reduceMotionStatusDidChangeNotification, object: nil)
        NotificationCenter.default.addObserver(self, selector: #selector(render), name: UIContentSizeCategory.didChangeNotification, object: nil)
        navigationItem.largeTitleDisplayMode = .never
        checkButton = UIBarButtonItem(title: "Check", image: UIImage(systemName: "arrow.clockwise"), primaryAction: UIAction { [weak self] _ in
            self?.perform { try await $0.check() }
        })
        checkButton.accessibilityLabel = "Check for agent updates"
        checkButton.accessibilityIdentifier = "updates-check"
        navigationItem.rightBarButtonItem = checkButton
        var config = UICollectionLayoutListConfiguration(appearance: .insetGrouped)
        config.backgroundColor = .clear
        list = UICollectionView(frame: .zero, collectionViewLayout: UICollectionViewCompositionalLayout.list(using: config))
        list.translatesAutoresizingMaskIntoConstraints = false
        list.backgroundColor = .clear
        list.accessibilityIdentifier = "agent-updates"
        view.addSubview(list)
        NSLayoutConstraint.activate([
            list.leadingAnchor.constraint(equalTo: view.leadingAnchor), list.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            list.topAnchor.constraint(equalTo: view.topAnchor), list.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
        let agentCell = UICollectionView.CellRegistration<AgentUpdateCell, String> { [weak self] cell, _, id in
            guard let self, let row = current?.rows.first(where: { $0.harness == id }) else { return }
            cell.configure(row, expanded: expanded.contains(id))
            cell.primary.addAction(UIAction(identifier: UIAction.Identifier("primary")) { [weak self] _ in
                self?.perform { h in
                    if row.canCancel { try await h.cancel(harness: id) } else { try await h.apply(harness: id) }
                }
            }, for: .touchUpInside)
            cell.policy.menu = policyMenu(row)
            cell.details.addAction(UIAction(identifier: UIAction.Identifier("details")) { [weak self] _ in
                guard let self else { return }
                if expanded.contains(id) { expanded.remove(id) } else { expanded.insert(id) }
                render()
            }, for: .touchUpInside)
            cell.copy.addAction(UIAction(identifier: UIAction.Identifier("copy")) { _ in UIPasteboard.general.string = row.manualCommand }, for: .touchUpInside)
        }
        let summaryCell = UICollectionView.CellRegistration<UICollectionViewListCell, String> { [weak self] cell, _, _ in
            guard let self, let s = current else { return }
            var content = UIListContentConfiguration.subtitleCell()
            content.text = s.deviceName
            content.textProperties.font = UIFontMetrics(forTextStyle: .headline).scaledFont(for: Fonts.ui(.sansMedium, 17))
            content.textProperties.color = Palette.text
            content.secondaryText = summary(s)
            content.secondaryTextProperties.numberOfLines = 0
            content.secondaryTextProperties.color = Palette.secondary
            content.image = UIImage(systemName: "desktopcomputer")
            content.imageProperties.tintColor = Palette.secondary
            cell.contentConfiguration = content
            var background = UIBackgroundConfiguration.listGroupedCell()
            background.backgroundColor = Palette.elevated
            cell.backgroundConfiguration = background
            cell.accessibilityIdentifier = "updates-host"
            if s.connection == .reconnecting {
                let button = UIButton(type: .system)
                button.setTitle("Retry", for: .normal)
                button.accessibilityIdentifier = "updates-retry"
                button.frame = CGRect(x: 0, y: 0, width: 60, height: 44)
                button.addAction(UIAction { [weak self] _ in self?.handle?.retry() }, for: .touchUpInside)
                cell.accessories = [.customView(configuration: .init(customView: button, placement: .trailing()))]
            } else if s.connection == .loading || s.checkPending {
                let glyph = StatusGlyph(.spinner)
                glyph.frame = CGRect(x: 0, y: 0, width: 24, height: 24)
                cell.accessories = [.customView(configuration: .init(customView: glyph, placement: .trailing()))]
            } else { cell.accessories = [] }
        }
        dataSource = UICollectionViewDiffableDataSource(collectionView: list) { cv, path, id in
            if id == "host" { return cv.dequeueConfiguredReusableCell(using: summaryCell, for: path, item: id) }
            return cv.dequeueConfiguredReusableCell(using: agentCell, for: path, item: id)
        }
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        do { handle = try app.client?.openAgentUpdates(deviceId: deviceId) }
        catch { showError(error) }
        observer = app.observeAgentUpdates(deviceId) { [weak self] in self?.render() }
        workspaceObserver = app.observe { [weak self] in self?.render() }
        render()
    }
    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        handle?.close()
        handle = nil
        observer = nil
        workspaceObserver = nil
    }

    deinit { NotificationCenter.default.removeObserver(self) }

    private func summary(_ s: AgentUpdateSnapshot) -> String {
        let text: String
        switch s.connection {
        case .ready: text = s.rows.isEmpty ? "No installed agents found on this device." : "Updates install on this device when its agents are idle."
        case .loading: text = "Connecting to this device…"
        case .offline: text = "Offline. Connect this device to manage its agents."
        case .unsupported: text = "Update Zeron on this device to manage agent updates."
        case .removed: text = "This device is no longer in your workspace."
        case .closed: text = "Sign in again to manage this device."
        case .reconnecting: text = "Reconnecting. Shown status may be out of date."
        }
        return [text, s.checkError, s.connection == .reconnecting ? s.watchError : nil].compactMap { $0 }.joined(separator: "\n\n")
    }
    @objc private func render() {
        guard let s = handle?.snapshot(), dataSource != nil else { return }
        current = s
        checkButton.isEnabled = s.connection == .ready && !s.checkPending
        checkButton.title = s.checkPending ? "Checking…" : "Check"
        var snapshot = NSDiffableDataSourceSnapshot<String, String>()
        snapshot.appendSections(["host", "agents"])
        snapshot.appendItems(["host"], toSection: "host")
        snapshot.appendItems(s.rows.map(\.harness), toSection: "agents")
        let previous = Set(dataSource.snapshot().itemIdentifiers)
        snapshot.reconfigureItems(snapshot.itemIdentifiers.filter { previous.contains($0) })
        dataSource.apply(snapshot, animatingDifferences: false)
    }
    private func policyMenu(_ row: AgentUpdateRow) -> UIMenu {
        let policies: [(String, AgentUpdatePolicy)] = [("Notify", .notify), ("Automatic when idle", .autoWhenIdle), ("Off", .off)]
        var children: [UIMenuElement] = [UIMenu(title: "Update policy", options: .displayInline, children: policies.map { name, policy in
            UIAction(title: name, attributes: row.canSetPolicy ? [] : .disabled, state: row.policy == policy ? .on : .off) { [weak self] _ in
                self?.perform { try await $0.setPolicy(harness: row.harness, policy: policy) }
            }
        })]
        if row.canDismiss {
            children.append(UIAction(title: "Dismiss this version", image: UIImage(systemName: "eye.slash")) { [weak self] _ in
                self?.perform { try await $0.dismiss(harness: row.harness) }
            })
        }
        return UIMenu(children: children)
    }
    private func perform(_ action: @escaping (AgentUpdatesHandle) async throws -> Void) {
        guard let handle else { return }
        Task { [weak self] in
            do { try await action(handle) }
            catch { if self?.viewIfLoaded?.window != nil { self?.showError(error) } }
            self?.render()
        }
    }
    private func showError(_ error: Error) {
        let message: String
        if let core = error as? CoreError {
            switch core {
            case let .NotFound(message: text), let .InvalidArgument(message: text),
                 let .HostUnavailable(message: text), let .Unsupported(message: text),
                 let .HostError(message: text), let .Network(message: text), let .Auth(message: text),
                 let .Storage(message: text), let .NotImplemented(message: text), let .Internal(message: text): message = text
            case .Closed: message = "Sign in again to manage this device."
            }
        } else { message = error.localizedDescription }
        let alert = UIAlertController(title: "Agent updates", message: message, preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: "OK", style: .default))
        present(alert, animated: true)
    }
}

private final class AgentUpdateCell: UICollectionViewListCell {
    let primary = UIButton(type: .system)
    let policy = UIButton(type: .system)
    let details = UIButton(type: .system)
    let copy = UIButton(type: .system)
    private let name = UILabel()
    private let version = UILabel()
    private let status = UILabel()
    private let message = UILabel()
    private let glyph = StatusGlyph()
    private let mark = UIImageView()
    private let root = UIStackView()
    private let controls = UIStackView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        let title = UIStackView(arrangedSubviews: [mark, name])
        title.spacing = 10
        title.alignment = .center
        let statusLine = UIStackView(arrangedSubviews: [glyph, status])
        statusLine.spacing = 8
        statusLine.alignment = .center
        [policy, UIView(), primary].forEach { controls.addArrangedSubview($0) }
        controls.alignment = .center
        controls.spacing = 12
        root.axis = .vertical
        root.spacing = 8
        [title, version, statusLine, message, details, copy, controls].forEach { root.addArrangedSubview($0) }
        root.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(root)
        NSLayoutConstraint.activate([
            root.leadingAnchor.constraint(equalTo: contentView.layoutMarginsGuide.leadingAnchor),
            root.trailingAnchor.constraint(equalTo: contentView.layoutMarginsGuide.trailingAnchor),
            root.topAnchor.constraint(equalTo: contentView.topAnchor, constant: 16),
            root.bottomAnchor.constraint(equalTo: contentView.bottomAnchor, constant: -12),
            mark.widthAnchor.constraint(equalToConstant: 22), mark.heightAnchor.constraint(equalToConstant: 22),
            glyph.widthAnchor.constraint(equalToConstant: 16), glyph.heightAnchor.constraint(equalToConstant: 18),
            primary.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
            policy.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
            details.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
            copy.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
        ])
        mark.contentMode = .scaleAspectFit
        mark.tintColor = Palette.text
        for label in [name, version, status, message] {
            label.numberOfLines = 0
            label.adjustsFontForContentSizeCategory = true
        }
        name.font = UIFontMetrics(forTextStyle: .headline).scaledFont(for: Fonts.ui(.sansMedium, 17))
        version.font = UIFontMetrics(forTextStyle: .subheadline).scaledFont(for: Fonts.ui(.mono, 13))
        status.font = UIFontMetrics(forTextStyle: .subheadline).scaledFont(for: Fonts.ui(.sans, 14))
        message.font = UIFontMetrics(forTextStyle: .footnote).scaledFont(for: Fonts.ui(.sans, 13))
        name.textColor = Palette.text
        version.textColor = Palette.secondary
        message.textColor = Palette.secondary
        policy.showsMenuAsPrimaryAction = true
        glyph.isAccessibilityElement = false
        mark.isAccessibilityElement = false
        var background = UIBackgroundConfiguration.listGroupedCell()
        background.backgroundColor = Palette.elevated
        backgroundConfiguration = background
    }
    required init?(coder: NSCoder) { fatalError() }
    func configure(_ row: AgentUpdateRow, expanded: Bool) {
        for (button, id) in [(primary, "primary"), (details, "details"), (copy, "copy")] {
            button.removeAction(identifiedBy: UIAction.Identifier(id), for: .touchUpInside)
        }
        controls.axis = traitCollection.preferredContentSizeCategory.isAccessibilityCategory ? .vertical : .horizontal
        controls.alignment = controls.axis == .vertical ? .leading : .center
        accessibilityIdentifier = "update-row-\(row.harness)"
        name.text = row.name
        mark.image = BrandMarks.image(for: row.harness, side: 22)
        if let latest = row.latestVersion {
            version.text = row.installedVersion.map { "\($0) → \(latest)" } ?? "Available: \(latest)"
        } else { version.text = row.installedVersion.map { "Installed \($0)" } }
        version.isHidden = version.text == nil
        let busy = [AgentUpdatePhase.checking, .waitingForIdle, .preparing, .downloading, .installing, .verifying].contains(row.phase)
        glyph.kind = busy || row.pending ? (UIAccessibility.isReduceMotionEnabled ? .dot(Palette.accent) : .spinner) : row.phase == .updated || row.phase == .current ? .check(Palette.success) : .dot(row.phase == .failed ? Palette.danger : Palette.accent)
        status.text = row.pending && row.phase == .available ? "Requesting update…" : row.progressMessage ?? row.phase.label
        status.textColor = row.phase == .failed ? Palette.danger : Palette.secondary
        status.accessibilityIdentifier = "update-status-\(row.harness)"
        message.text = row.error ?? (expanded ? row.manualCommand : nil)
        message.isHidden = message.text == nil
        message.numberOfLines = 0
        details.isHidden = row.manualCommand == nil
        details.setTitle(expanded ? "Hide instructions" : "Show instructions", for: .normal)
        details.contentHorizontalAlignment = .leading
        copy.isHidden = !expanded || row.manualCommand == nil
        copy.setTitle("Copy instructions", for: .normal)
        copy.contentHorizontalAlignment = .leading
        var config = UIButton.Configuration.tinted()
        config.title = row.canCancel ? "Cancel" : row.phase == .failed ? "Retry" : "Update"
        config.baseForegroundColor = Palette.accent
        config.cornerStyle = .capsule
        config.contentInsets = NSDirectionalEdgeInsets(top: 8, leading: 16, bottom: 8, trailing: 16)
        primary.configuration = config
        primary.isHidden = !row.canApply && !row.canCancel && !row.pending
        primary.isEnabled = row.canApply || row.canCancel
        primary.accessibilityIdentifier = "update-action-\(row.harness)"
        primary.accessibilityLabel = "\(config.title!) \(row.name)"
        var policyConfig = UIButton.Configuration.plain()
        policyConfig.title = row.policy == .notify ? "Notify" : row.policy == .off ? "Off" : "Automatic"
        policyConfig.image = UIImage(systemName: "chevron.down", withConfiguration: UIImage.SymbolConfiguration(pointSize: 10, weight: .medium))
        policyConfig.imagePlacement = .trailing
        policyConfig.imagePadding = 6
        policyConfig.baseForegroundColor = Palette.secondary
        policyConfig.contentInsets = .zero
        policy.configuration = policyConfig
        policy.isEnabled = row.canSetPolicy
        policy.accessibilityIdentifier = "update-policy-\(row.harness)"
        policy.accessibilityLabel = "\(row.name) update policy"
        policy.accessibilityValue = policyConfig.title
    }
}
private extension AgentUpdatePhase {
    var label: String {
        switch self {
        case .dormant: "Monitoring is off"
        case .checking: "Checking for updates…"
        case .current: "Up to date"
        case .available: "Update available"
        case .waitingForIdle: "Waiting for this agent to finish"
        case .preparing: "Preparing update…"
        case .downloading: "Downloading update…"
        case .installing: "Installing update…"
        case .verifying: "Verifying installation…"
        case .updated: "Updated"
        case .manualActionRequired: "Manual update required"
        case .failed: "Update failed"
        }
    }
}
