import Foundation

/// The game's save blob is a handful of bytes; UserDefaults is plenty.
enum SaveStore {
    private static let key = "topple.save"

    static func load() -> Data? {
        UserDefaults.standard.data(forKey: key)
    }

    static func store(_ data: Data) {
        UserDefaults.standard.set(data, forKey: key)
    }
}
