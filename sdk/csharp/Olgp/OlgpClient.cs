using System.Net.Http.Json;
using System.Net.WebSockets;
using System.Text;
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
    private readonly string? _token;
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
        _token = credential;
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
        Audio = new AudioRoutes(this);
        Engine = new EngineRoutes(this);
        Realtime = new RealtimeRoutes(this);
    }

    public SpeakerRoutes Speakers { get; }
    public TimerRoutes Timers { get; }
    public MemoRoutes Memos { get; }
    public PhoneRoutes Phones { get; }
    public AudioMemoryRoutes AudioMemory { get; }
    public KnowledgeBaseRoutes KnowledgeBases { get; }
    public ImageRoutes Images { get; }
    public RecordingRoutes AudioRecordings { get; }
    public AudioRoutes Audio { get; }
    public EngineRoutes Engine { get; }
    public RealtimeRoutes Realtime { get; }

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

    private async Task<byte[]> GetBytesAsync(string path) =>
        (await GetBytesWithContentTypeAsync(path).ConfigureAwait(false)).Data;

    private async Task<(byte[] Data, string ContentType)> GetBytesWithContentTypeAsync(string path)
    {
        using var response = await _http.GetAsync(path).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var data = await response.Content.ReadAsByteArrayAsync().ConfigureAwait(false);
        return (data, response.Content.Headers.ContentType?.ToString() ?? "application/octet-stream");
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

            public Task<(byte[] Data, string ContentType)> ResultBytesAsync(string jobId) =>
                client.GetBytesWithContentTypeAsync($"/v2/images/jobs/{Uri.EscapeDataString(jobId)}/result");

            public Task<string> SaveResultAsync((byte[] Data, string ContentType) result, string path) =>
                SaveBytesAsync(result.Data, path);

            public async Task<string> SaveResultAsync(string jobId, string path)
            {
                var (data, _) = await ResultBytesAsync(jobId).ConfigureAwait(false);
                return await SaveBytesAsync(data, path).ConfigureAwait(false);
            }

            private static async Task<string> SaveBytesAsync(byte[] data, string path)
            {
                await File.WriteAllBytesAsync(path, data).ConfigureAwait(false);
                return path;
            }

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

        public Task<JsonElement> SessionsCreateAsync(string model, string task) =>
            client.SendAsync(HttpMethod.Post, "/engine/sessions", new { model, task });
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

    public sealed class AudioRoutes(OlgpClient client)
    {
        public RecordingCaptureRoutes Recordings { get; } = new(client);
    }

    public sealed class RecordingCaptureRoutes(OlgpClient client)
    {
        public Task<JsonElement> StartAsync(object? input = null) =>
            client.SendAsync(
                HttpMethod.Post,
                "/v1/audio/recordings",
                input ?? new { retainAudio = false }
            );

        public Task<JsonElement> AppendAsync(string recordingId, uint seq, string pcm) =>
            client.SendAsync(
                HttpMethod.Post,
                $"/v1/audio/recordings/{Uri.EscapeDataString(recordingId)}/audio",
                new { seq, pcm }
            );

        public async Task<JsonElement> AppendResumableAsync(string recordingId, string pcm)
        {
            var recording = await ResumeAsync(recordingId).ConfigureAwait(false);
            var seq = recording.ValueKind == JsonValueKind.Object
                && recording.TryGetProperty("chunk_count", out var count)
                && count.TryGetUInt32(out var next)
                    ? next
                    : 0u;
            return await AppendAsync(recordingId, seq, pcm).ConfigureAwait(false);
        }

        public Task<JsonElement> ResumeAsync(string recordingId) =>
            client.SendAsync(
                HttpMethod.Get,
                $"/v1/audio/recordings/{Uri.EscapeDataString(recordingId)}"
            );
    }

    public sealed class RealtimeRoutes(OlgpClient client)
    {
        public Task<RealtimeSession> ConnectAsync(
            string mode,
            CancellationToken cancellationToken = default) =>
            RealtimeSession.ConnectAsync(client, mode, cancellationToken);
    }

    public sealed class RealtimeSession
    {
        private static readonly byte[] CommitFrame = "{\"type\":\"commit\"}"u8.ToArray();
        private static readonly byte[] CancelFrame = "{\"type\":\"cancel\"}"u8.ToArray();
        private static readonly byte[] ResetFrame = "{\"type\":\"reset\"}"u8.ToArray();

        private readonly ClientWebSocket _socket;
        private readonly SemaphoreSlim _sendLock = new(1, 1);
        private readonly object _eventLock = new();
        private readonly Queue<JsonElement> _pendingEvents = new();
        private readonly Task _receiveTask;
        private Action<JsonElement>? _onEvent;
        private int _closed;

        public Action<JsonElement>? OnEvent
        {
            get { lock (_eventLock) return _onEvent; }
            set
            {
                JsonElement[] pending;
                lock (_eventLock)
                {
                    _onEvent = value;
                    if (value is null)
                    {
                        return;
                    }
                    pending = _pendingEvents.ToArray();
                    _pendingEvents.Clear();
                }
                foreach (var item in pending)
                {
                    value(item);
                }
            }
        }

        public Action<ReadOnlyMemory<byte>>? OnAudio { get; set; }

        private RealtimeSession(ClientWebSocket socket)
        {
            _socket = socket;
            _receiveTask = ReceiveLoopAsync();
            _ = _receiveTask.ContinueWith(
                static task => _ = task.Exception,
                CancellationToken.None,
                TaskContinuationOptions.OnlyOnFaulted | TaskContinuationOptions.ExecuteSynchronously,
                TaskScheduler.Default);
        }

        internal static async Task<RealtimeSession> ConnectAsync(
            OlgpClient client,
            string mode,
            CancellationToken cancellationToken)
        {
            if (mode is not ("voice" or "chat"))
            {
                throw new ArgumentOutOfRangeException(nameof(mode), mode, "mode must be 'voice' or 'chat'");
            }
            var builder = new UriBuilder(client._http.BaseAddress!)
            {
                Path = "/v1/realtime",
                Query = $"mode={Uri.EscapeDataString(mode)}&token={Uri.EscapeDataString(client._token ?? "")}"
            };
            builder.Scheme = builder.Scheme switch
            {
                "https" => "wss",
                "http" => "ws",
                _ => builder.Scheme
            };
            var socket = new ClientWebSocket();
            try
            {
                await socket.ConnectAsync(builder.Uri, cancellationToken).ConfigureAwait(false);
                return new RealtimeSession(socket);
            }
            catch
            {
                socket.Dispose();
                throw;
            }
        }

        public Task SendPcmAsync(ReadOnlyMemory<byte> pcm, CancellationToken cancellationToken = default)
        {
            var frame = new byte[pcm.Length + 1];
            frame[0] = 0x01;
            pcm.CopyTo(frame.AsMemory(1));
            return SendAsync(frame, WebSocketMessageType.Binary, cancellationToken);
        }

        public Task SendTextAsync(string text, CancellationToken cancellationToken = default) =>
            SendAsync(
                JsonSerializer.SerializeToUtf8Bytes(new { type = "text_input", text }),
                WebSocketMessageType.Text,
                cancellationToken);

        public Task CommitAsync(CancellationToken cancellationToken = default) =>
            SendAsync(CommitFrame, WebSocketMessageType.Text, cancellationToken);

        public Task CancelAsync(CancellationToken cancellationToken = default) =>
            SendAsync(CancelFrame, WebSocketMessageType.Text, cancellationToken);

        public Task ResetAsync(CancellationToken cancellationToken = default) =>
            SendAsync(ResetFrame, WebSocketMessageType.Text, cancellationToken);

        public async Task CloseAsync(CancellationToken cancellationToken = default)
        {
            if (Interlocked.Exchange(ref _closed, 1) != 0)
            {
                return;
            }
            try
            {
                if (_socket.State == WebSocketState.Open)
                {
                    await _socket.CloseOutputAsync(
                        WebSocketCloseStatus.NormalClosure,
                        "client closed",
                        cancellationToken).ConfigureAwait(false);
                }
            }
            catch (WebSocketException) { }
            catch (OperationCanceledException) { }
            _socket.Abort();
            _socket.Dispose();
        }

        private async Task SendAsync(
            ReadOnlyMemory<byte> payload,
            WebSocketMessageType messageType,
            CancellationToken cancellationToken)
        {
            ThrowIfClosed();
            await _sendLock.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                ThrowIfClosed();
                await _socket.SendAsync(payload, messageType, endOfMessage: true, cancellationToken)
                    .ConfigureAwait(false);
            }
            finally
            {
                _sendLock.Release();
            }
        }

        private void ThrowIfClosed()
        {
            if (Volatile.Read(ref _closed) != 0 || _socket.State != WebSocketState.Open)
            {
                throw new InvalidOperationException("realtime session is closed");
            }
        }

        private async Task ReceiveLoopAsync()
        {
            var buffer = new byte[64 * 1024];
            try
            {
                while (_socket.State == WebSocketState.Open)
                {
                    using var frame = new MemoryStream();
                    WebSocketReceiveResult result;
                    do
                    {
                        result = await _socket.ReceiveAsync(new ArraySegment<byte>(buffer), CancellationToken.None)
                            .ConfigureAwait(false);
                        if (result.MessageType == WebSocketMessageType.Close)
                        {
                            return;
                        }
                        frame.Write(buffer, 0, result.Count);
                    } while (!result.EndOfMessage);

                    var payload = frame.ToArray();
                    if (result.MessageType == WebSocketMessageType.Text)
                    {
                        DispatchEvent(OlgpClient.Parse(Encoding.UTF8.GetString(payload)));
                    }
                    else if (result.MessageType == WebSocketMessageType.Binary
                        && payload.Length > 0
                        && payload[0] == 0x01)
                    {
                        OnAudio?.Invoke(new ReadOnlyMemory<byte>(payload, 1, payload.Length - 1));
                    }
                }
            }
            catch (WebSocketException) { }
            catch (ObjectDisposedException) { }
            catch (OperationCanceledException) { }
        }

        private void DispatchEvent(JsonElement item)
        {
            Action<JsonElement>? handler;
            lock (_eventLock)
            {
                handler = _onEvent;
                if (handler is null)
                {
                    _pendingEvents.Enqueue(item);
                    return;
                }
            }
            handler(item);
        }
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
