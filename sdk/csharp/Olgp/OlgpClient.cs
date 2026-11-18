using System.Net.Http.Json;
using System.Text.Json.Serialization;

namespace Olgp;

public sealed record ChatMessage(string Role, string Content);

internal sealed record ChatRequest(string Model, ChatMessage[] Messages);

internal sealed record ChatResponse(Choice[] Choices);

internal sealed record Choice(MessageWrapper Message);

internal sealed record MessageWrapper(string Content);

/// <summary>Minimal client for the OLGP OpenAI-compatible endpoint.</summary>
public sealed class OlgpClient
{
    private readonly HttpClient _http;

    public OlgpClient(string? baseUrl = null, string? token = null)
    {
        var address = baseUrl
            ?? Environment.GetEnvironmentVariable("OLGP_BASE_URL")
            ?? "http://olgp.local:8787";
        _http = new HttpClient { BaseAddress = new Uri(address) };
        var credential = token ?? Environment.GetEnvironmentVariable("OLGP_DEVICE_TOKEN") ?? "";
        _http.DefaultRequestHeaders.Add("Authorization", $"Bearer {credential}");
    }

    public async Task<string> ChatAsync(string model, IEnumerable<ChatMessage> messages)
    {
        var response = await _http.PostAsJsonAsync(
            "/v1/chat/completions",
            new ChatRequest(model, messages.ToArray())
        );
        response.EnsureSuccessStatusCode();
        var body = await response.Content.ReadFromJsonAsync<ChatResponse>();
        return body?.Choices.FirstOrDefault()?.Message.Content
            ?? throw new InvalidOperationException("OLGP returned no choices");
    }
}

public static class Olgp
{
    public static OlgpClient Connect(string? baseUrl = null, string? token = null) =>
        new(baseUrl, token);
}
