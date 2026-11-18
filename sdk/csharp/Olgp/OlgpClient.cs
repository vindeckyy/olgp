using System.Net.Http.Json;
using System.Text.Json;
using System.Runtime.CompilerServices;

namespace Olgp;

public sealed record ChatMessage(string Role, string Content);

internal sealed record ChatRequest(string Model, ChatMessage[] Messages);

internal sealed record ChatResponse(Choice[] Choices);

internal sealed record Choice(MessageWrapper Message);

internal sealed record MessageWrapper(string Content);

public sealed class OlgpClient
{
    private readonly HttpClient _http;
    private readonly string? _memoryToken;

    public OlgpClient(
        string? baseUrl = null,
        string? token = null,
        string? memoryToken = null,
        string? searchProvider = null,
        string? tavilyKey = null)
    {
        var address = baseUrl
            ?? Environment.GetEnvironmentVariable("OLGP_BASE_URL")
            ?? "http://olgp.local:8787";
        _http = new HttpClient { BaseAddress = new Uri(address) };
        var credential = token ?? Environment.GetEnvironmentVariable("OLGP_DEVICE_TOKEN") ?? "";
        _http.DefaultRequestHeaders.Add("Authorization", $"Bearer {credential}");
        _memoryToken = memoryToken ?? Environment.GetEnvironmentVariable("OLGP_MEMORY_TOKEN");
        searchProvider ??= Environment.GetEnvironmentVariable("OLGP_SEARCH_PROVIDER");
        tavilyKey ??= Environment.GetEnvironmentVariable("OLGP_TAVILY_KEY")
            ?? Environment.GetEnvironmentVariable("TAVILY_API_KEY");
        if (!string.IsNullOrEmpty(searchProvider))
        {
            _http.DefaultRequestHeaders.Add("X-Wici-Search-Provider", searchProvider);
        }
        if (!string.IsNullOrEmpty(tavilyKey))
        {
            _http.DefaultRequestHeaders.Add("X-Wici-Tavily-Key", tavilyKey);
        }
        Speakers = new SpeakerRoutes(this);
        Timers = new TimerRoutes(this);
        Memos = new MemoRoutes(this);
        Phones = new PhoneRoutes(this);
        AudioMemory = new AudioMemoryRoutes(this);
        KnowledgeBases = new KnowledgeBaseRoutes(this);
        Images = new ImageRoutes(this);
        AudioRecordings = new RecordingRoutes(this);
        Engine = new EngineRoutes(this);
    }

    public SpeakerRoutes Speakers { get; }
    public TimerRoutes Timers { get; }
    public MemoRoutes Memos { get; }
    public PhoneRoutes Phones { get; }
    public AudioMemoryRoutes AudioMemory { get; }
    public KnowledgeBaseRoutes KnowledgeBases { get; }
    public ImageRoutes Images { get; }
    public RecordingRoutes AudioRecordings { get; }
    public EngineRoutes Engine { get; }

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

    private async Task<JsonElement> SendAsync(
        HttpMethod method,
        string path,
        object? body = null,
        bool memory = false)
    {
        using var request = new HttpRequestMessage(method, path);
        if (body is not null)
        {
            request.Content = JsonContent.Create(body);
        }
        if (memory && _memoryToken is not null)
        {
            request.Headers.Add("X-Wici-Memory-Token", _memoryToken);
        }
        using var response = await _http.SendAsync(request).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        return Parse(await response.Content.ReadAsStringAsync().ConfigureAwait(false));
    }

    private async Task<byte[]> GetBytesAsync(string path)
    {
        using var response = await _http.GetAsync(path).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        return await response.Content.ReadAsByteArrayAsync().ConfigureAwait(false);
    }

    private static JsonElement Parse(string content)
    {
        using var document = JsonDocument.Parse(
            string.IsNullOrWhiteSpace(content) ? "null" : content
        );
        return document.RootElement.Clone();
    }

    public sealed class SpeakerRoutes(OlgpClient client)
    {
        public Task<JsonElement> ListAsync() => client.SendAsync(HttpMethod.Get, "/v1/speakers");

        public Task<JsonElement> RetrieveAsync(string speakerId) =>
            client.SendAsync(HttpMethod.Get, $"/v1/speakers/{Uri.EscapeDataString(speakerId)}");

        public Task<JsonElement> UpdateAsync(string speakerId, int volume) =>
            client.SendAsync(
                HttpMethod.Patch,
                $"/v1/speakers/{Uri.EscapeDataString(speakerId)}",
                new { volume }
            );

        public Task<JsonElement> PlayAsync(string speakerId, string url) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/speakers/{Uri.EscapeDataString(speakerId)}:play",
                new { url }
            );

        public Task<JsonElement> StopAsync(string speakerId) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/speakers/{Uri.EscapeDataString(speakerId)}:stop"
            );

        public Task<JsonElement> BluetoothAsync(string speakerId) =>
            client.SendAsync(
                HttpMethod.Get,
                $"/v1/speakers/{Uri.EscapeDataString(speakerId)}/bluetooth"
            );

        public Task<JsonElement> StartBluetoothPairingAsync(string speakerId) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/speakers/{Uri.EscapeDataString(speakerId)}:startBluetoothPairing"
            );
    }

    public sealed class TimerRoutes(OlgpClient client)
    {
        public Task<JsonElement> ListAsync() => client.SendAsync(HttpMethod.Get, "/v1/timers");

        public Task<JsonElement> CreateAsync(
            string? title = null,
            long dueAtMs = 0,
            string kind = "timer",
            string? deviceId = null) =>
            client.SendAsync(
                HttpMethod.Post,
                "/v1/timers",
                new { kind, dueAtMs, title, deviceId }
            );

        public Task<JsonElement> CancelAsync(string timerId) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/timers/{Uri.EscapeDataString(timerId)}:cancel"
            );

        public Task<JsonElement> SnoozeAsync(string timerId, long delaySec = 60) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/timers/{Uri.EscapeDataString(timerId)}:snooze",
                new { delaySec }
            );
    }

    public sealed class MemoRoutes(OlgpClient client)
    {
        public Task<JsonElement> ListAsync() => client.SendAsync(HttpMethod.Get, "/v1/memos");

        public Task<JsonElement> CreateAsync(string? title = null, string? deviceId = null) =>
            client.SendAsync(HttpMethod.Post, "/v1/memos", new { title, deviceId });

        public Task<JsonElement> DismissAsync(string memoId) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/memos/{Uri.EscapeDataString(memoId)}:dismiss"
            );
    }

    public sealed class PhoneRoutes(OlgpClient client)
    {
        public Task<JsonElement> RingAsync(string phoneId, uint durationSec = 5) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/phones/{Uri.EscapeDataString(phoneId)}:ring",
                new { durationSec }
            );
    }

    public sealed class AudioMemoryRoutes(OlgpClient client)
    {
        public Task<JsonElement> StatusAsync() =>
            client.SendAsync(HttpMethod.Get, "/v1/audio-memory/status", memory: true);

        public Task<JsonElement> ReadyAsync() =>
            client.SendAsync(HttpMethod.Get, "/v1/audio-memory/health/ready", memory: true);

        public Task<JsonElement> SettingsAsync() =>
            client.SendAsync(HttpMethod.Get, "/v1/audio-memory/settings", memory: true);

        public Task<JsonElement> UpdateSettingsAsync(string mode, uint retentionDays) =>
            client.SendAsync(
                HttpMethod.Patch,
                "/v1/audio-memory/settings",
                new { mode, retention_days = retentionDays },
                memory: true
            );

        public Task<JsonElement> EpisodesAsync() =>
            client.SendAsync(HttpMethod.Get, "/v1/audio-memory/episodes", memory: true);

        public Task<JsonElement> NotesAsync() =>
            client.SendAsync(HttpMethod.Get, "/v1/audio-memory/notes", memory: true);

        public Task<JsonElement> AddNoteAsync(string text, string kind = "fact") =>
            client.SendAsync(
                HttpMethod.Post,
                "/v1/audio-memory/notes",
                new { text, kind },
                memory: true
            );

        public Task<JsonElement> QueryAsync(string question) =>
            client.SendAsync(
                HttpMethod.Post,
                "/v1/audio-memory:query",
                new { question },
                memory: true
            );

        public Task<JsonElement> ForgetAsync(string id) =>
            client.SendAsync(
                HttpMethod.Post,
                "/v1/audio-memory:forget",
                new { id },
                memory: true
            );

        public Task<JsonElement> ClearAsync() =>
            client.SendAsync(HttpMethod.Post, "/v1/audio-memory:clear", memory: true);
    }

    public sealed class KnowledgeBaseRoutes(OlgpClient client)
    {
        public Task<JsonElement> CreateAsync(string name) =>
            client.SendAsync(HttpMethod.Post, "/v1/knowledge-bases", new { name });

        public Task<JsonElement> RetrieveAsync(string knowledgeBaseId) =>
            client.SendAsync(
                HttpMethod.Get,
                $"/v1/knowledge-bases/{Uri.EscapeDataString(knowledgeBaseId)}"
            );

        public Task<JsonElement> DeleteAsync(string knowledgeBaseId) =>
            client.SendAsync(
                HttpMethod.Delete,
                $"/v1/knowledge-bases/{Uri.EscapeDataString(knowledgeBaseId)}"
            );

        public Task<JsonElement> DocumentsAsync(string knowledgeBaseId) =>
            client.SendAsync(
                HttpMethod.Get,
                $"/v1/knowledge-bases/{Uri.EscapeDataString(knowledgeBaseId)}/documents"
            );

        public Task<JsonElement> AddTextDocumentAsync(
            string knowledgeBaseId,
            string filename,
            string text) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/knowledge-bases/{Uri.EscapeDataString(knowledgeBaseId)}/documents:text",
                new { filename, text }
            );

        public Task<JsonElement> AddFileDocumentAsync(
            string knowledgeBaseId,
            string filename,
            string content,
            string mime = "application/octet-stream") =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/knowledge-bases/{Uri.EscapeDataString(knowledgeBaseId)}/documents:file",
                new { filename, content, mime }
            );

        public Task<JsonElement> AddImageDocumentAsync(
            string knowledgeBaseId,
            string filename,
            string value) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/knowledge-bases/{Uri.EscapeDataString(knowledgeBaseId)}/documents:image",
                new { filename, value }
            );

        public Task<JsonElement> QueryAsync(
            string knowledgeBaseId,
            string query,
            int topK = 5) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/knowledge-bases/{Uri.EscapeDataString(knowledgeBaseId)}:query",
                new { query, topK }
            );
    }

    public sealed class ImageRoutes(OlgpClient client)
    {
        public JobRoutes Jobs { get; } = new(client);

        public Task<JsonElement> CreateAsync(object? request = null) =>
            client.SendAsync(
                HttpMethod.Post,
                "/v2/images/jobs",
                request ?? new { operation = "generate" }
            );

        public Task<JsonElement> RetrieveAsync(string jobId) =>
            client.SendAsync(
                HttpMethod.Get,
                $"/v2/images/jobs/{Uri.EscapeDataString(jobId)}"
            );

        public Task<byte[]> ResultAsync(string jobId) =>
            client.GetBytesAsync($"/v2/images/jobs/{Uri.EscapeDataString(jobId)}/result");

        public Task<JsonElement> CancelAsync(string jobId) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v2/images/jobs/{Uri.EscapeDataString(jobId)}:cancel"
            );

        public async IAsyncEnumerable<JsonElement> StreamAsync<T>(
            IEnumerable<T> frames,
            object? options = null,
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            foreach (var frame in frames)
            {
                cancellationToken.ThrowIfCancellationRequested();
                yield return await ProcessFrameAsync(frame, options).ConfigureAwait(false);
            }
        }

        public async IAsyncEnumerable<JsonElement> StreamAsync<T>(
            IAsyncEnumerable<T> frames,
            object? options = null,
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            await foreach (var frame in frames
                .WithCancellation(cancellationToken)
                .ConfigureAwait(false))
            {
                yield return await ProcessFrameAsync(frame, options).ConfigureAwait(false);
            }
        }

        private Task<JsonElement> ProcessFrameAsync(object frame, object? options) =>
            client.SendAsync(HttpMethod.Post, "/v2/images/frames:process", new { frame, options });

        public sealed class JobRoutes(OlgpClient client)
        {
            public Task<JsonElement> CreateAsync(object? request = null) =>
                client.SendAsync(HttpMethod.Post, "/v2/images/jobs", request ?? new { operation = "generate" });

            public Task<JsonElement> RetrieveAsync(string jobId) =>
                client.SendAsync(
                    HttpMethod.Get,
                    $"/v2/images/jobs/{Uri.EscapeDataString(jobId)}"
                );

            public Task<byte[]> ResultAsync(string jobId) =>
                client.GetBytesAsync($"/v2/images/jobs/{Uri.EscapeDataString(jobId)}/result");

            public Task<JsonElement> CancelAsync(string jobId) =>
                client.SendAsync(
                    HttpMethod.Post,
                    $"/v2/images/jobs/{Uri.EscapeDataString(jobId)}:cancel"
                );

            public async IAsyncEnumerable<JsonElement> ObserveAsync(
                string jobId,
                int intervalMs = 500,
                [EnumeratorCancellation] CancellationToken cancellationToken = default)
            {
                while (true)
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    var state = await RetrieveAsync(jobId).ConfigureAwait(false);
                    yield return state;
                    if (IsTerminal(state))
                    {
                        yield break;
                    }
                    await Task.Delay(intervalMs, cancellationToken).ConfigureAwait(false);
                }
            }

            public async Task<JsonElement> WaitAsync(
                string jobId,
                int intervalMs = 500,
                int timeoutMs = 600_000,
                CancellationToken cancellationToken = default)
            {
                using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
                timeout.CancelAfter(timeoutMs);
                JsonElement state = default;
                try
                {
                    await foreach (var update in ObserveAsync(jobId, intervalMs, timeout.Token)
                        .WithCancellation(timeout.Token)
                        .ConfigureAwait(false))
                    {
                        state = update;
                    }
                }
                catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
                {
                    throw new TimeoutException($"timed out waiting for image job {jobId}");
                }
                var status = Status(state);
                if (status != "succeeded")
                {
                    throw new InvalidOperationException(
                        $"image job {jobId} ended with status {status ?? "unknown"}");
                }
                return state;
            }

            private static string? Status(JsonElement state) =>
                state.ValueKind == JsonValueKind.Object
                && state.TryGetProperty("status", out var value)
                && value.ValueKind == JsonValueKind.String
                    ? value.GetString()
                    : null;

            private static bool IsTerminal(JsonElement state) =>
                Status(state) is "succeeded" or "failed" or "cancelled"
                    or "model_missing" or "deployment_unavailable"
                    or "gpu_lifecycle_unavailable" or "error";
        }
    }

    public sealed class EngineRoutes(OlgpClient client)
    {
        public Task<JsonElement> StatusAsync() =>
            client.SendAsync(HttpMethod.Get, "/engine/status");

        public Task<JsonElement> CapabilitiesAsync() =>
            client.SendAsync(HttpMethod.Get, "/engine/capabilities");

        public Task<JsonElement> SessionsAsync() =>
            client.SendAsync(HttpMethod.Get, "/engine/sessions");
    }

    public sealed class RecordingRoutes(OlgpClient client)
    {
        public Task<JsonElement> CreateAsync(bool retainAudio = false) =>
            client.SendAsync(
                HttpMethod.Post,
                "/v1/audio/recordings",
                new { retainAudio }
            );

        public Task<JsonElement> ListAsync() =>
            client.SendAsync(HttpMethod.Get, "/v1/audio/recordings");

        public Task<JsonElement> GetAsync(string recordingId) =>
            client.SendAsync(
                HttpMethod.Get,
                $"/v1/audio/recordings/{Uri.EscapeDataString(recordingId)}"
            );

        public Task<JsonElement> DeleteAsync(string recordingId) =>
            client.SendAsync(
                HttpMethod.Delete,
                $"/v1/audio/recordings/{Uri.EscapeDataString(recordingId)}"
            );

        public Task<JsonElement> AppendAsync(string recordingId, uint seq, string pcm) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/audio/recordings/{Uri.EscapeDataString(recordingId)}/audio",
                new { seq, pcm }
            );

        public Task<JsonElement> FinishAsync(string recordingId) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/audio/recordings/{Uri.EscapeDataString(recordingId)}:finish"
            );

        public Task<JsonElement> ReprocessAsync(string recordingId) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/audio/recordings/{Uri.EscapeDataString(recordingId)}:reprocess"
            );
    }
}

public static class Olgp
{
    public static OlgpClient Connect(
        string? baseUrl = null,
        string? token = null,
        string? memoryToken = null,
        string? searchProvider = null,
        string? tavilyKey = null) =>
        new(baseUrl, token, memoryToken, searchProvider, tavilyKey);
}
