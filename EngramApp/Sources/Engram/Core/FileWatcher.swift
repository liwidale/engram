import Foundation

final class FileWatcher: @unchecked Sendable {
    private let url: URL
    private let queue = DispatchQueue(label: "com.liwidale.engram.file-watcher")
    private let onChange: @MainActor () -> Void
    private let lock = NSLock()
    private var source: DispatchSourceFileSystemObject?
    private var pending: DispatchWorkItem?
    private var stopped = false

    init(url: URL, onChange: @escaping @MainActor () -> Void) {
        self.url = url
        self.onChange = onChange
    }

    deinit { stop() }

    func start() {
        queue.async { [weak self] in self?.arm(attempt: 0) }
    }

    func stop() {
        lock.lock()
        stopped = true
        pending?.cancel()
        source?.cancel()
        source = nil
        lock.unlock()
    }

    private func arm(attempt: Int) {
        lock.lock()
        defer { lock.unlock() }
        guard !stopped else { return }
        source?.cancel()
        source = nil
        let fd = open(url.path, O_EVTONLY)
        guard fd >= 0 else {
            if attempt < 40 {
                queue.asyncAfter(deadline: .now() + .milliseconds(min(50 * (attempt + 1), 1000))) { [weak self] in
                    self?.arm(attempt: attempt + 1)
                }
            }
            return
        }
        let src = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd, eventMask: [.write, .extend, .delete, .rename, .attrib, .revoke], queue: queue
        )
        src.setEventHandler { [weak self, weak src] in
            guard let self, let src else { return }
            let events = src.data
            self.schedule()
            if !events.isDisjoint(with: [.delete, .rename, .revoke]) {
                self.queue.asyncAfter(deadline: .now() + .milliseconds(20)) { [weak self] in self?.arm(attempt: 0) }
            }
        }
        src.setCancelHandler { close(fd) }
        source = src
        src.resume()
        if attempt > 0 { scheduleLocked() }
    }

    private func schedule() {
        lock.lock()
        defer { lock.unlock() }
        scheduleLocked()
    }

    private func scheduleLocked() {
        guard !stopped else { return }
        pending?.cancel()
        let onChange = onChange
        let work = DispatchWorkItem {
            DispatchQueue.main.async { MainActor.assumeIsolated { onChange() } }
        }
        pending = work
        queue.asyncAfter(deadline: .now() + .milliseconds(90), execute: work)
    }
}
