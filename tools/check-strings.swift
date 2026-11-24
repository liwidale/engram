import Foundation

let args = CommandLine.arguments.dropFirst()
guard args.count == 2, let tableURL = args.last.map(URL.init(fileURLWithPath:)) else {
    FileHandle.standardError.write(Data("usage: check-strings <stringsdata-dir> <Localizable.strings>\n".utf8))
    exit(2)
}
let dir = URL(fileURLWithPath: args.first!)
let format = try NSRegularExpression(pattern: "%(?:lld|llu|@|%)")

func specifiers(_ s: String) -> [String] {
    format.matches(in: s, range: NSRange(s.startIndex..., in: s)).map { (s as NSString).substring(with: $0.range) }
}

var keys = Set<String>()
for file in try FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)
where file.pathExtension == "stringsdata" {
    let json = try JSONSerialization.jsonObject(with: Data(contentsOf: file)) as? [String: Any]
    let tables = json?["tables"] as? [String: [[String: Any]]] ?? [:]
    for entry in tables["Localizable"] ?? [] {
        if let key = entry["key"] as? String, !key.isEmpty { keys.insert(key) }
    }
}
guard let table = NSDictionary(contentsOf: tableURL) as? [String: String] else {
    FileHandle.standardError.write(Data("cannot read \(tableURL.path)\n".utf8))
    exit(1)
}
let missing = keys.subtracting(table.keys).sorted()
let unused = Set(table.keys).subtracting(keys).sorted()
let mismatched = table.filter { keys.contains($0.key) && specifiers($0.key) != specifiers($0.value) }.keys.sorted()
for key in missing { print("missing: \(key)") }
for key in unused { print("unused: \(key)") }
for key in mismatched { print("format differs: \(key)") }
if !(missing.isEmpty && unused.isEmpty && mismatched.isEmpty) { exit(1) }
print("\(keys.count) strings translated in \(tableURL.deletingLastPathComponent().lastPathComponent)")
