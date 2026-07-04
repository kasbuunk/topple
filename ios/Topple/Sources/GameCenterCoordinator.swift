import GameKit
import UIKit

/// Everything Game Center: sign-in, the turn-based matchmaker, and shipping
/// the Rust core's match blobs. The core is the source of truth for game
/// state; this class only moves bytes and turns.
final class GameCenterCoordinator: NSObject {
    private weak var host: UIViewController?
    private var currentMatch: GKTurnBasedMatch?
    private var pendingLevel: UInt32 = 2

    init(host: UIViewController) {
        self.host = host
        super.init()
    }

    // ---------------------------------------------------------------- auth --

    func authenticate() {
        GKLocalPlayer.local.authenticateHandler = { [weak self] viewController, error in
            guard let self else { return }
            if let viewController {
                self.host?.present(viewController, animated: true)
                return
            }
            if GKLocalPlayer.local.isAuthenticated {
                GKLocalPlayer.local.register(self)
                topple_set_online_available(1)
            } else {
                topple_set_online_available(0)
                if let error { print("Game Center: \(error.localizedDescription)") }
            }
        }
    }

    // --------------------------------------------------------- matchmaking --

    func startMatchmaking(level: UInt32) {
        pendingLevel = level
        let request = GKMatchRequest()
        request.minPlayers = 2
        request.maxPlayers = 2
        request.inviteMessage = "Topple: one formula, two sides. Play me."
        let matchmaker = GKTurnBasedMatchmakerViewController(matchRequest: request)
        matchmaker.turnBasedMatchmakerDelegate = self
        host?.present(matchmaker, animated: true)
    }

    private var localIsP1: Bool {
        guard let match = currentMatch else { return false }
        // The creator is always the first participant.
        return match.participants.first?.player == GKLocalPlayer.local
    }

    private var opponents: [GKTurnBasedParticipant] {
        currentMatch?.participants.filter { $0.player != GKLocalPlayer.local } ?? []
    }

    /// A match became active (created, selected, or its turn arrived):
    /// fetch its authoritative data and hand it to the core.
    private func activate(_ match: GKTurnBasedMatch) {
        currentMatch = match
        match.loadMatchData { [weak self] data, error in
            guard let self else { return }
            if let error { print("loadMatchData: \(error.localizedDescription)") }
            guard self.currentMatch?.matchID == match.matchID else { return }
            self.ingest(data ?? Data(), into: match)
        }
    }

    private func ingest(_ data: Data, into match: GKTurnBasedMatch) {
        if data.isEmpty {
            // A brand-new match: we created it, so we are P1 and the core
            // deals from a fresh seed at the difficulty the player chose.
            var seed: UInt64 = 0
            arc4random_buf(&seed, MemoryLayout<UInt64>.size)
            topple_online_create(
                UInt32(truncatingIfNeeded: seed),
                UInt32(truncatingIfNeeded: seed >> 32),
                pendingLevel,
                1
            )
            // The header ships on the next frame via the outbox poll.
            return
        }
        data.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
            guard let src = raw.baseAddress else { return }
            let dst = topple_inbox_alloc(UInt32(data.count))
            dst?.update(from: src.assumingMemoryBound(to: UInt8.self), count: data.count)
        }
        if topple_online_load(localIsP1 ? 1 : 0) == 0 {
            print("Game Center: match data was rejected by the core")
        }
    }

    // ------------------------------------------------------------- shipping --

    /// Fresh bytes from the core's outbox. Whose input comes next decides
    /// whether we hold the turn, pass it, or end the match.
    func ship(data: Data, status: UInt32) {
        guard let match = currentMatch else { return }
        switch status {
        case 1: // still the local player's input (e.g. picked the tempo, moves first)
            match.saveCurrentTurn(withMatch: data) { error in
                if let error { print("saveCurrentTurn: \(error.localizedDescription)") }
            }
        case 2: // the opponent acts next
            match.endTurn(
                withNextParticipants: opponents,
                turnTimeout: GKTurnTimeoutDefault,
                match: data
            ) { error in
                if let error { print("endTurn: \(error.localizedDescription)") }
            }
        case 3, 4: // the winning move was just made locally
            for participant in match.participants {
                let isLocal = participant.player == GKLocalPlayer.local
                let localWon = status == 3
                participant.matchOutcome = (isLocal == localWon) ? .won : .lost
            }
            match.endMatchInTurn(withMatch: data) { error in
                if let error { print("endMatchInTurn: \(error.localizedDescription)") }
            }
        default:
            break
        }
    }

    func resignCurrentMatch() {
        guard let match = currentMatch else { return }
        let data = match.matchData ?? Data()
        if match.currentParticipant?.player == GKLocalPlayer.local {
            match.participantQuitInTurn(
                with: .quit,
                nextParticipants: opponents,
                turnTimeout: GKTurnTimeoutDefault,
                match: data
            ) { _ in }
        } else {
            match.participantQuitOutOfTurn(with: .quit) { _ in }
        }
        currentMatch = nil
    }
}

// ------------------------------------------------------- matchmaker sheet --

extension GameCenterCoordinator: GKTurnBasedMatchmakerViewControllerDelegate {
    func turnBasedMatchmakerViewControllerWasCancelled(
        _ viewController: GKTurnBasedMatchmakerViewController
    ) {
        viewController.dismiss(animated: true)
    }

    func turnBasedMatchmakerViewController(
        _ viewController: GKTurnBasedMatchmakerViewController,
        didFailWithError error: Error
    ) {
        print("matchmaker: \(error.localizedDescription)")
        viewController.dismiss(animated: true)
    }
}

// ----------------------------------------------------------- turn events --

extension GameCenterCoordinator: GKLocalPlayerListener {
    func player(
        _ player: GKPlayer,
        receivedTurnEventFor match: GKTurnBasedMatch,
        didBecomeActive: Bool
    ) {
        if didBecomeActive {
            // The player chose this match (matchmaker or notification).
            host?.presentedViewController?.dismiss(animated: true)
            activate(match)
            return
        }
        // Background chatter: only refresh the match already on screen —
        // never yank the player out of whatever they are playing.
        guard currentMatch?.matchID == match.matchID else { return }
        activate(match)
    }

    func player(_ player: GKPlayer, matchEnded match: GKTurnBasedMatch) {
        guard currentMatch?.matchID == match.matchID else { return }
        // Sync the final position first, then tell the core if it still
        // thought the game was running (the opponent quit).
        match.loadMatchData { [weak self] data, _ in
            guard let self else { return }
            self.ingest(data ?? Data(), into: match)
            let status = topple_online_status()
            if status == 1 || status == 2 {
                topple_online_opponent_quit()
            }
            self.currentMatch = nil
        }
    }

    /// The local player quit this match from the matchmaker list.
    func player(_ player: GKPlayer, wantsToQuitMatch match: GKTurnBasedMatch) {
        match.participantQuitInTurn(
            with: .quit,
            nextParticipants: match.participants.filter { $0.player != GKLocalPlayer.local },
            turnTimeout: GKTurnTimeoutDefault,
            match: match.matchData ?? Data()
        ) { _ in }
        if currentMatch?.matchID == match.matchID {
            currentMatch = nil
        }
    }
}
