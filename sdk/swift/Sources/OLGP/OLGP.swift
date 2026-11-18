import Foundation

public struct ChatMessage: Codable, Sendable {
    public let role: String
    public let content: String

    public init(role: String, content: String) {
        self.role = role
        self.content = content
    }
}

public struct ChatCompletionRequest: Codable, Sendable {
    let model: String
    let messages: [ChatMessage]
}

struct ChatCompletionResponse: Codable {
    struct Choice: Codable {
        struct Message: Codable {
            let content: String
        }
        let message: Message
    }
    let choices: [Choice]
}

public enum OLGPUtilsError: Error {
    case invalidURL
    case badResponse
    case jobTimeout
}

public final class Client {
    private let baseURL: URL
    private let token: String
    private let memoryToken: String
    private let searchProvider: String?
    private let tavilyKey: String?

    public init(
        baseURL: String? = nil,
        token: String? = nil,
        memoryToken: String? = nil,
        searchProvider: String? = nil,
        tavilyKey: String? = nil
    ) throws {
        let value = baseURL ?? ProcessInfo.processInfo.environment["OLGP_BASE_URL"]
            ?? "http://olgp.local:8787"
        guard let url = URL(string: value) else {
            throw OLGPUtilsError.invalidURL
        }
        self.baseURL = url
        self.token = token ?? ProcessInfo.processInfo.environment["OLGP_DEVICE_TOKEN"] ?? ""
        self.memoryToken = memoryToken ?? ProcessInfo.processInfo.environment["OLGP_MEMORY_TOKEN"] ?? ""
        let env = ProcessInfo.processInfo.environment
        self.searchProvider = searchProvider ?? env["OLGP_SEARCH_PROVIDER"]
        self.tavilyKey = tavilyKey ?? env["TAVILY_API_KEY"] ?? env["OLGP_TAVILY_KEY"]
    }

    private func applyAuthHeaders(_ request: inout URLRequest) {
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        if let searchProvider = searchProvider, !searchProvider.isEmpty {
            request.setValue(searchProvider, forHTTPHeaderField: "X-Wici-Search-Provider")
        }
        if let tavilyKey = tavilyKey, !tavilyKey.isEmpty {
            request.setValue(tavilyKey, forHTTPHeaderField: "X-Wici-Tavily-Key")
        }
    }

    public func chat(model: String, messages: [ChatMessage]) async throws -> String {
        var request = URLRequest(url: baseURL.appendingPathComponent("/v1/chat/completions"))
        request.httpMethod = "POST"
        applyAuthHeaders(&request)
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(
            ChatCompletionRequest(model: model, messages: messages)
        )
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            throw OLGPUtilsError.badResponse
        }
        let decoded = try JSONDecoder().decode(ChatCompletionResponse.self, from: data)
        guard let first = decoded.choices.first else {
            throw OLGPUtilsError.badResponse
        }
        return first.message.content
    }

    private func request(
        _ method: String,
        _ path: String,
        body: [String: Any]? = nil,
        memory: Bool = false
    ) async throws -> Data {
        let response = try await requestWithContentType(method, path, body: body, memory: memory)
        return response.data
    }

    private func requestWithContentType(
        _ method: String,
        _ path: String,
        body: [String: Any]? = nil,
        memory: Bool = false
    ) async throws -> (data: Data, contentType: String?) {
        guard let url = URL(string: path, relativeTo: baseURL)?.absoluteURL else {
            throw OLGPUtilsError.invalidURL
        }
        var request = URLRequest(url: url)
        request.httpMethod = method
        applyAuthHeaders(&request)
        if memory, !memoryToken.isEmpty {
            request.setValue(memoryToken, forHTTPHeaderField: "X-Wici-Memory-Token")
        }
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            throw OLGPUtilsError.badResponse
        }
        return (data, http.value(forHTTPHeaderField: "Content-Type"))
    }

    private func object(
        _ method: String,
        _ path: String,
        body: [String: Any]? = nil,
        memory: Bool = false
    ) async throws -> [String: Any] {
        let data = try await request(method, path, body: body, memory: memory)
        guard let value = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw OLGPUtilsError.badResponse
        }
        return value
    }

    private func list(
        _ method: String,
        _ path: String,
        body: [String: Any]? = nil,
        memory: Bool = false
    ) async throws -> [[String: Any]] {
        let data = try await request(method, path, body: body, memory: memory)
        guard let value = try JSONSerialization.jsonObject(with: data) as? [[String: Any]] else {
            throw OLGPUtilsError.badResponse
        }
        return value
    }

    // MARK: - Speakers

    public func speakersList() async throws -> [[String: Any]] {
        try await list("GET", "/v1/speakers")
    }

    public func speakersRetrieve(_ speakerId: String) async throws -> [String: Any] {
        try await object("GET", "/v1/speakers/\(speakerId)")
    }

    public func speakersUpdate(_ speakerId: String, volume: Int) async throws -> [String: Any] {
        try await object("PATCH", "/v1/speakers/\(speakerId)", body: ["volume": volume])
    }

    public func speakersPlay(_ speakerId: String, url: String) async throws -> [String: Any] {
        try await object("POST", "/v1/speakers/\(speakerId):play", body: ["url": url])
    }

    public func speakersStop(_ speakerId: String) async throws -> [String: Any] {
        try await object("POST", "/v1/speakers/\(speakerId):stop")
    }

    public func speakersBluetooth(_ speakerId: String) async throws -> [String: Any] {
        try await object("GET", "/v1/speakers/\(speakerId)/bluetooth")
    }

    public func speakersStartBluetoothPairing(_ speakerId: String) async throws -> [String: Any] {
        try await object("POST", "/v1/speakers/\(speakerId):startBluetoothPairing")
    }

    // MARK: - Timers

    public func timersList() async throws -> [[String: Any]] {
        try await list("GET", "/v1/timers")
    }

    public func timersCreate(
        kind: String? = nil,
        dueAtMs: Int? = nil,
        title: String? = nil,
        deviceId: String? = nil
    ) async throws -> [String: Any] {
        var body: [String: Any] = [:]
        if let kind = kind { body["kind"] = kind }
        if let dueAtMs = dueAtMs { body["dueAtMs"] = dueAtMs }
        if let title = title { body["title"] = title }
        if let deviceId = deviceId { body["deviceId"] = deviceId }
        return try await object("POST", "/v1/timers", body: body)
    }

    public func timersCancel(_ timerId: String) async throws -> [String: Any] {
        try await object("POST", "/v1/timers/\(timerId):cancel")
    }

    public func timersSnooze(_ timerId: String, delaySec: Int = 60) async throws -> [String: Any] {
        try await object("POST", "/v1/timers/\(timerId):snooze", body: ["delaySec": delaySec])
    }

    // MARK: - Memos

    public func memosList() async throws -> [[String: Any]] {
        try await list("GET", "/v1/memos")
    }

    public func memosCreate(title: String, deviceId: String? = nil) async throws -> [String: Any] {
        var body: [String: Any] = ["title": title]
        if let deviceId = deviceId { body["deviceId"] = deviceId }
        return try await object("POST", "/v1/memos", body: body)
    }

    public func memosDismiss(_ memoId: String) async throws -> [String: Any] {
        try await object("POST", "/v1/memos/\(memoId):dismiss")
    }

    // MARK: - Phones

    public func phonesRing(_ phoneId: String, durationSec: Int = 5) async throws -> [String: Any] {
        try await object("POST", "/v1/phones/\(phoneId):ring", body: ["durationSec": durationSec])
    }

    // MARK: - Audio memory

    public func audioMemoryStatus() async throws -> [String: Any] {
        try await object("GET", "/v1/audio-memory/status", memory: true)
    }

    public func audioMemoryReady() async throws -> [String: Any] {
        try await object("GET", "/v1/audio-memory/health/ready", memory: true)
    }

    public func audioMemorySettings() async throws -> [String: Any] {
        try await object("GET", "/v1/audio-memory/settings", memory: true)
    }

    public func audioMemorySetSettings(
        mode: String? = nil,
        retentionDays: Int? = nil
    ) async throws -> [String: Any] {
        var body: [String: Any] = [:]
        if let mode = mode { body["mode"] = mode }
        if let retentionDays = retentionDays { body["retention_days"] = retentionDays }
        return try await object("PATCH", "/v1/audio-memory/settings", body: body, memory: true)
    }

    public func audioMemoryEpisodes() async throws -> [[String: Any]] {
        try await list("GET", "/v1/audio-memory/episodes", memory: true)
    }

    public func audioMemoryNotes() async throws -> [[String: Any]] {
        try await list("GET", "/v1/audio-memory/notes", memory: true)
    }

    public func audioMemoryAddNote(text: String, kind: String? = nil) async throws -> [String: Any] {
        var body: [String: Any] = ["text": text]
        if let kind = kind { body["kind"] = kind }
        return try await object("POST", "/v1/audio-memory/notes", body: body, memory: true)
    }

    public func audioMemoryQuery(question: String) async throws -> [String: Any] {
        try await object("POST", "/v1/audio-memory:query", body: ["question": question], memory: true)
    }

    public func audioMemoryForget(id: String) async throws -> [String: Any] {
        try await object("POST", "/v1/audio-memory:forget", body: ["id": id], memory: true)
    }

    public func audioMemoryClear() async throws -> [String: Any] {
        try await object("POST", "/v1/audio-memory:clear", memory: true)
    }

    // MARK: - Knowledge bases

    public func knowledgeBasesCreate(name: String) async throws -> [String: Any] {
        try await object("POST", "/v1/knowledge-bases", body: ["name": name])
    }

    public func knowledgeBasesRetrieve(_ knowledgeBaseId: String) async throws -> [String: Any] {
        try await object("GET", "/v1/knowledge-bases/\(knowledgeBaseId)")
    }

    public func knowledgeBasesDelete(_ knowledgeBaseId: String) async throws -> [String: Any] {
        try await object("DELETE", "/v1/knowledge-bases/\(knowledgeBaseId)")
    }

    public func knowledgeBasesDocuments(_ knowledgeBaseId: String) async throws -> [String: Any] {
        try await object("GET", "/v1/knowledge-bases/\(knowledgeBaseId)/documents")
    }

    public func knowledgeBasesAddText(
        _ knowledgeBaseId: String,
        text: String,
        filename: String = "document.txt"
    ) async throws -> [String: Any] {
        try await object(
            "POST",
            "/v1/knowledge-bases/\(knowledgeBaseId)/documents:text",
            body: ["filename": filename, "text": text]
        )
    }

    public func knowledgeBasesAddFile(
        _ knowledgeBaseId: String,
        content: String,
        filename: String = "file.bin",
        mime: String = "application/octet-stream"
    ) async throws -> [String: Any] {
        try await object(
            "POST",
            "/v1/knowledge-bases/\(knowledgeBaseId)/documents:file",
            body: ["filename": filename, "mime": mime, "content": content]
        )
    }

    public func knowledgeBasesAddImage(
        _ knowledgeBaseId: String,
        value: String,
        filename: String = "image.png"
    ) async throws -> [String: Any] {
        try await object(
            "POST",
            "/v1/knowledge-bases/\(knowledgeBaseId)/documents:image",
            body: ["filename": filename, "value": value]
        )
    }

    public func knowledgeBasesQuery(
        _ knowledgeBaseId: String,
        query: String,
        topK: Int = 5
    ) async throws -> [String: Any] {
        try await object(
            "POST",
            "/v1/knowledge-bases/\(knowledgeBaseId):query",
            body: ["query": query, "topK": topK]
        )
    }

    // MARK: - Engine

    public func engineStatus() async throws -> [String: Any] {
        try await object("GET", "/engine/status")
    }

    public func engineCapabilities() async throws -> [String: Any] {
        try await object("GET", "/engine/capabilities")
    }

    public func engineSessions() async throws -> [String: Any] {
        try await object("GET", "/engine/sessions")
    }

    public func engineSessionsCreate(model: String, task: String) async throws -> [String: Any] {
        try await object("POST", "/engine/sessions", body: ["model": model, "task": task])
    }

    // MARK: - Image jobs

    public func imagesJobsCreate(
        operation: String = "generate",
        payload: [String: Any] = [:]
    ) async throws -> [String: Any] {
        var body = payload
        body["operation"] = operation
        return try await object("POST", "/v2/images/jobs", body: body)
    }

    public func imagesJobsRetrieve(_ jobId: String) async throws -> [String: Any] {
        try await object("GET", "/v2/images/jobs/\(jobId)")
    }

    public func imagesJobsResult(_ jobId: String) async throws -> Data {
        try await request("GET", "/v2/images/jobs/\(jobId)/result")
    }

    public func imageJobResultBytes(_ jobId: String) async throws -> Data {
        try await request("GET", "/v2/images/jobs/\(jobId)/result")
    }

    public func imageJobResultBytesWithContentType(
        _ jobId: String
    ) async throws -> (data: Data, contentType: String?) {
        try await requestWithContentType("GET", "/v2/images/jobs/\(jobId)/result")
    }

    public func imagesJobsCancel(_ jobId: String) async throws -> [String: Any] {
        try await object("POST", "/v2/images/jobs/\(jobId):cancel")
    }

    public func imagesFramesProcess(
        frames: [String],
        options: [String: Any] = [:]
    ) async throws -> [String: Any] {
        var body = options
        body["frames"] = frames
        return try await object("POST", "/v2/images/frames:process", body: body)
    }

    public func imagesStream(
        frames: [String],
        options: [String: Any] = [:]
    ) async throws -> [String: Any] {
        try await imagesFramesProcess(frames: frames, options: options)
    }

    // shortcut: polls the job route; the contract has no job-events subscription.
    public func imagesJobsObserve(
        _ jobId: String,
        intervalMs: UInt64 = 250,
        timeoutMs: UInt64 = 60_000
    ) async throws -> [String: Any] {
        let deadline = Date().addingTimeInterval(Double(timeoutMs) / 1_000)
        while true {
            let job = try await imagesJobsRetrieve(jobId)
            let status = (job["status"] as? String)?.lowercased() ?? ""
            if ["succeeded", "failed", "canceled", "cancelled", "model_missing"].contains(status) {
                return job
            }
            guard Date() < deadline else { throw OLGPUtilsError.jobTimeout }
            try await Task.sleep(nanoseconds: intervalMs * 1_000_000)
        }
    }

    public func imagesJobsWait(
        _ jobId: String,
        intervalMs: UInt64 = 250,
        timeoutMs: UInt64 = 60_000
    ) async throws -> Data {
        let job = try await imagesJobsObserve(jobId, intervalMs: intervalMs, timeoutMs: timeoutMs)
        guard (job["status"] as? String)?.lowercased() == "succeeded" else {
            throw OLGPUtilsError.badResponse
        }
        return try await imagesJobsResult(jobId)
    }

    // MARK: - Recordings

    public func audioRecordingsCreate(retainAudio: Bool = false) async throws -> [String: Any] {
        try await object("POST", "/v1/audio/recordings", body: ["retainAudio": retainAudio])
    }

    public func audioRecordingsList() async throws -> [[String: Any]] {
        try await list("GET", "/v1/audio/recordings")
    }

    public func audioRecordingsGet(_ recordingId: String) async throws -> [String: Any] {
        try await object("GET", "/v1/audio/recordings/\(recordingId)")
    }

    public func audioRecordingsDelete(_ recordingId: String) async throws -> [String: Any] {
        try await object("DELETE", "/v1/audio/recordings/\(recordingId)")
    }

    public func audioRecordingsAppend(
        _ recordingId: String,
        seq: Int,
        pcm: String
    ) async throws -> [String: Any] {
        try await object(
            "POST",
            "/v1/audio/recordings/\(recordingId)/audio",
            body: ["seq": seq, "pcm": pcm]
        )
    }

    public func audioRecordingsFinish(_ recordingId: String) async throws -> [String: Any] {
        try await object("POST", "/v1/audio/recordings/\(recordingId):finish")
    }

    public func audioRecordingsReprocess(_ recordingId: String) async throws -> [String: Any] {
        try await object("POST", "/v1/audio/recordings/\(recordingId):reprocess")
    }

    // MARK: - Recording capture handles

    public func audioRecordingCaptureStart(
        name: String? = nil,
        language: String? = nil,
        retainAudio: Bool = false,
        audio: [String: Any]? = nil
    ) async throws -> [String: Any] {
        var body: [String: Any] = ["retainAudio": retainAudio]
        if let name = name { body["name"] = name }
        if let language = language { body["language"] = language }
        if let audio = audio { body["audio"] = audio }
        let response = try await object("POST", "/v1/audio/recordings", body: body)
        return response["result"] as? [String: Any] ?? response
    }

    public func audioRecordingCaptureAppend(
        _ recordingId: String,
        seq: Int,
        pcm: String
    ) async throws -> [String: Any] {
        let response = try await object(
            "POST",
            "/v1/audio/recordings/\(recordingId)/audio",
            body: ["seq": seq, "pcm": pcm]
        )
        return response["result"] as? [String: Any] ?? response
    }

    public func audioRecordingCaptureAppendResumable(
        _ recordingId: String,
        pcm: String
    ) async throws -> [String: Any] {
        // shortcut: seq comes from accepted chunk_count; retrying an acked chunk duplicates it.
        let recording = try await audioRecordingsGet(recordingId)
        let seq = recording["chunk_count"] as? Int ?? 0
        return try await audioRecordingCaptureAppend(recordingId, seq: seq, pcm: pcm)
    }

    public func audioRecordingCaptureResume(_ recordingId: String) async throws -> [String: Any] {
        try await audioRecordingsGet(recordingId)
    }

}

public func connect(
    baseURL: String? = nil,
    token: String? = nil,
    memoryToken: String? = nil,
    searchProvider: String? = nil,
    tavilyKey: String? = nil
) throws -> Client {
    try Client(
        baseURL: baseURL,
        token: token,
        memoryToken: memoryToken,
        searchProvider: searchProvider,
        tavilyKey: tavilyKey
    )
}
