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
}

public final class Client {
    private let baseURL: URL
    private let token: String

    public init(baseURL: String? = nil, token: String? = nil) throws {
        let value = baseURL ?? ProcessInfo.processInfo.environment["OLGP_BASE_URL"]
            ?? "http://olgp.local:8787"
        guard let url = URL(string: value) else {
            throw OLGPUtilsError.invalidURL
        }
        self.baseURL = url
        self.token = token ?? ProcessInfo.processInfo.environment["OLGP_DEVICE_TOKEN"] ?? ""
    }

    public func chat(model: String, messages: [ChatMessage]) async throws -> String {
        var request = URLRequest(url: baseURL.appendingPathComponent("/v1/chat/completions"))
        request.httpMethod = "POST"
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
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
}

public func connect(baseURL: String? = nil, token: String? = nil) throws -> Client {
    try Client(baseURL: baseURL, token: token)
}
