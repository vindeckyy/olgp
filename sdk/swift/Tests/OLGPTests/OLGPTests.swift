import XCTest
@testable import OLGP

final class OLGPTests: XCTestCase {
    func testChatMessageEncoding() throws {
        let message = ChatMessage(role: "user", content: "hello")
        let data = try JSONEncoder().encode(message)
        let decoded = try JSONDecoder().decode(ChatMessage.self, from: data)
        XCTAssertEqual(decoded.content, "hello")
    }

    func testClientUsesEnvironmentDefaults() throws {
        let client = try Client()
        XCTAssertNotNil(client)
    }
}
