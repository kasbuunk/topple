import UIKit

/// The whole app is one screen: the Rust core's 640×480 framebuffer,
/// letterboxed, with touch mapped onto it and a few physical-feeling
/// buttons in the pillars. All game logic lives across the C ABI.
final class GameViewController: UIViewController {
    private let fbView = FramebufferView()
    private var displayLink: CADisplayLink?
    private var lastTimestamp: CFTimeInterval = 0
    private var gameCenter: GameCenterCoordinator?

    override var prefersStatusBarHidden: Bool { true }
    override var prefersHomeIndicatorAutoHidden: Bool { true }
    override var supportedInterfaceOrientations: UIInterfaceOrientationMask { .landscape }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(red: 0.055, green: 0.055, blue: 0.078, alpha: 1)

        bootCore()
        layoutGame()
        addControls()
        addGestures()

        gameCenter = GameCenterCoordinator(host: self)
        gameCenter?.authenticate()

        let link = CADisplayLink(target: self, selector: #selector(tick(_:)))
        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    private func bootCore() {
        var seed: UInt64 = 0
        arc4random_buf(&seed, MemoryLayout<UInt64>.size)
        let parts = Calendar.current.dateComponents([.year, .month, .day], from: Date())
        topple_boot(
            UInt32(truncatingIfNeeded: seed),
            UInt32(truncatingIfNeeded: seed >> 32),
            UInt32(parts.year ?? 2026),
            UInt32(parts.month ?? 1),
            UInt32(parts.day ?? 1),
            0 // online switches on once Game Center signs in
        )
        if let save = SaveStore.load() {
            save.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
                guard let src = raw.baseAddress else { return }
                let dst = topple_inbox_alloc(UInt32(save.count))
                dst?.update(from: src.assumingMemoryBound(to: UInt8.self), count: save.count)
            }
            topple_inbox_load_save()
        }
    }

    private func layoutGame() {
        fbView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(fbView)
        let safe = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            fbView.centerXAnchor.constraint(equalTo: safe.centerXAnchor),
            fbView.centerYAnchor.constraint(equalTo: safe.centerYAnchor),
            fbView.widthAnchor.constraint(equalTo: fbView.heightAnchor, multiplier: 4.0 / 3.0),
            fbView.heightAnchor.constraint(lessThanOrEqualTo: safe.heightAnchor),
            fbView.widthAnchor.constraint(lessThanOrEqualTo: safe.widthAnchor, constant: -176),
        ])
        let grow = fbView.heightAnchor.constraint(equalTo: safe.heightAnchor)
        grow.priority = .defaultHigh
        grow.isActive = true
    }

    // ------------------------------------------------------------ controls --

    private func addControls() {
        // Right pillar mirrors the game's mnemonic: ⊤ on top, ⊥ below.
        let top = padButton("⊤", code: 6, tint: UIColor(red: 0.96, green: 0.62, blue: 0.25, alpha: 1))
        let bot = padButton("⊥", code: 5, tint: UIColor(red: 0.38, green: 0.65, blue: 0.98, alpha: 1))
        let right = UIStackView(arrangedSubviews: [top, bot])
        right.axis = .vertical
        right.spacing = 24

        // Left pillar: the meta keys.
        let menu = padButton("☰", code: 8, tint: .lightGray, small: true)
        let zoom = padButton("⌕", code: 4, tint: .lightGray, small: true)
        let peek = padButton("◎", code: 7, tint: .lightGray, small: true)
        let left = UIStackView(arrangedSubviews: [menu, zoom, peek])
        left.axis = .vertical
        left.spacing = 18

        for stack in [left, right] {
            stack.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(stack)
        }
        let safe = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            right.trailingAnchor.constraint(equalTo: safe.trailingAnchor, constant: -12),
            right.centerYAnchor.constraint(equalTo: safe.centerYAnchor),
            left.leadingAnchor.constraint(equalTo: safe.leadingAnchor, constant: 12),
            left.centerYAnchor.constraint(equalTo: safe.centerYAnchor),
        ])
    }

    private func padButton(_ label: String, code: UInt32, tint: UIColor, small: Bool = false) -> UIButton {
        let b = UIButton(type: .system)
        b.setTitle(label, for: .normal)
        b.titleLabel?.font = .monospacedSystemFont(ofSize: small ? 24 : 34, weight: .bold)
        b.setTitleColor(tint, for: .normal)
        b.backgroundColor = UIColor(white: 1, alpha: 0.08)
        let side: CGFloat = small ? 52 : 64
        b.layer.cornerRadius = side / 2
        b.widthAnchor.constraint(equalToConstant: side).isActive = true
        b.heightAnchor.constraint(equalToConstant: side).isActive = true
        b.tag = Int(code)
        b.addTarget(self, action: #selector(padDown(_:)), for: .touchDown)
        for event: UIControl.Event in [.touchUpInside, .touchUpOutside, .touchCancel] {
            b.addTarget(self, action: #selector(padUp(_:)), for: event)
        }
        return b
    }

    @objc private func padDown(_ sender: UIButton) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        topple_key(UInt32(sender.tag), 1)
    }

    @objc private func padUp(_ sender: UIButton) {
        topple_key(UInt32(sender.tag), 0)
    }

    private func addGestures() {
        let tap = UITapGestureRecognizer(target: self, action: #selector(onTap(_:)))
        fbView.addGestureRecognizer(tap)
        let directions: [UISwipeGestureRecognizer.Direction] = [.up, .down, .left, .right]
        for direction in directions {
            let swipe = UISwipeGestureRecognizer(target: self, action: #selector(onSwipe(_:)))
            swipe.direction = direction
            view.addGestureRecognizer(swipe)
        }
    }

    @objc private func onTap(_ g: UITapGestureRecognizer) {
        let p = g.location(in: fbView)
        guard fbView.bounds.width > 0, fbView.bounds.height > 0 else { return }
        let fx = Float(p.x / fbView.bounds.width) * Float(topple_fb_width())
        let fy = Float(p.y / fbView.bounds.height) * Float(topple_fb_height())
        topple_tap(fx, fy)
    }

    @objc private func onSwipe(_ g: UISwipeGestureRecognizer) {
        let code: UInt32
        switch g.direction {
        case .up: code = 0
        case .down: code = 1
        case .left: code = 2
        case .right: code = 3
        default: return
        }
        topple_key(code, 1)
        topple_key(code, 0)
    }

    // ---------------------------------------------------------------- loop --

    @objc private func tick(_ link: CADisplayLink) {
        let dt: CFTimeInterval = lastTimestamp == 0 ? 1.0 / 60.0 : link.timestamp - lastTimestamp
        lastTimestamp = link.timestamp
        topple_frame(UInt32((dt * 1000).rounded()))
        fbView.present()

        // Persist saves the moment they change.
        let saveLen = topple_save_poll()
        if saveLen > 0, let ptr = topple_save_ptr() {
            SaveStore.store(Data(bytes: ptr, count: Int(saveLen)))
        }

        // Online plumbing: requests, resignations, and outgoing match data.
        let requestedLevel = topple_online_request_poll()
        if requestedLevel > 0 {
            gameCenter?.startMatchmaking(level: requestedLevel)
        }
        if topple_online_resign_poll() != 0 {
            gameCenter?.resignCurrentMatch()
        }
        let outLen = topple_online_outbox_poll()
        if outLen > 0, let ptr = topple_online_outbox_ptr() {
            let data = Data(bytes: ptr, count: Int(outLen))
            gameCenter?.ship(data: data, status: topple_online_status())
        }
    }
}

/// Letterboxed view whose layer shows the RGBA framebuffer, pixel-crisp.
final class FramebufferView: UIView {
    override init(frame: CGRect) {
        super.init(frame: frame)
        layer.magnificationFilter = .nearest
        layer.contentsGravity = .resize
        isUserInteractionEnabled = true
    }

    required init?(coder: NSCoder) { fatalError("no storyboards here") }

    func present() {
        guard let ptr = topple_fb_ptr() else { return }
        let width = Int(topple_fb_width())
        let height = Int(topple_fb_height())
        let data = Data(bytes: ptr, count: width * height * 4)
        guard let provider = CGDataProvider(data: data as CFData),
              let image = CGImage(
                  width: width,
                  height: height,
                  bitsPerComponent: 8,
                  bitsPerPixel: 32,
                  bytesPerRow: width * 4,
                  space: CGColorSpaceCreateDeviceRGB(),
                  bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
                  provider: provider,
                  decode: nil,
                  shouldInterpolate: false,
                  intent: .defaultIntent
              )
        else { return }
        layer.contents = image
    }
}
