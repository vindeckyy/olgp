type Json = Record<string, unknown>;
type DocumentsApi = {
  (knowledgeBaseId: string, payload?: Json): Promise<unknown>;
  list(knowledgeBaseId: string): Promise<unknown>;
  text(knowledgeBaseId: string, payload: Json): Promise<unknown>;
  file(knowledgeBaseId: string, payload: Json): Promise<unknown>;
  image(knowledgeBaseId: string, payload: Json): Promise<unknown>;
};

export interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

export interface OlgpClientOptions {
  baseUrl?: string;
  token?: string;
  memoryToken?: string;
}

export class OlgpClient {
  private readonly baseUrl: string;
  private readonly token: string;
  private readonly memoryToken: string;

  constructor(options: OlgpClientOptions = {}) {
    this.baseUrl = (options.baseUrl ?? process.env.OLGP_BASE_URL ?? "http://olgp.local:8787").replace(/\/$/, "");
    this.token = options.token ?? process.env.OLGP_DEVICE_TOKEN ?? "";
    this.memoryToken = options.memoryToken ?? process.env.OLGP_MEMORY_TOKEN ?? "";
  }

  private async request<T = unknown>(
    method: string,
    path: string,
    payload?: Json,
    memory = false,
  ): Promise<T> {
    const headers: Record<string, string> = {
      Authorization: `Bearer ${this.token}`,
    };
    if (memory) {
      headers["X-Wici-Memory-Token"] = this.memoryToken;
    }
    if (payload !== undefined) {
      headers["Content-Type"] = "application/json";
    }
    const response = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers,
      body: payload === undefined ? undefined : JSON.stringify(payload),
    });
    if (!response.ok) {
      throw new Error(`olgp: HTTP ${response.status}`);
    }
    if (response.status === 204) {
      return undefined as T;
    }
    return (await response.json()) as T;
  }

  readonly speakers = {
    list: () => this.request("GET", "/v1/speakers"),
    retrieve: (speakerId: string) => this.request("GET", `/v1/speakers/${encodeURIComponent(speakerId)}`),
    update: (speakerId: string, payload: Json) =>
      this.request("PATCH", `/v1/speakers/${encodeURIComponent(speakerId)}`, payload),
    play: (speakerId: string, payload?: Json) =>
      this.request("POST", `/v1/speakers/${encodeURIComponent(speakerId)}:play`, payload),
    stop: (speakerId: string) => this.request("POST", `/v1/speakers/${encodeURIComponent(speakerId)}:stop`),
    bluetooth: (speakerId: string) =>
      this.request("GET", `/v1/speakers/${encodeURIComponent(speakerId)}/bluetooth`),
    startBluetoothPairing: (speakerId: string) =>
      this.request("POST", `/v1/speakers/${encodeURIComponent(speakerId)}:startBluetoothPairing`),
  };

  readonly timers = {
    list: () => this.request("GET", "/v1/timers"),
    create: (payload: Json) => this.request("POST", "/v1/timers", payload),
    cancel: (timerId: string) => this.request("POST", `/v1/timers/${encodeURIComponent(timerId)}:cancel`),
    snooze: (timerId: string, payload?: Json) =>
      this.request("POST", `/v1/timers/${encodeURIComponent(timerId)}:snooze`, payload),
  };

  readonly memos = {
    list: () => this.request("GET", "/v1/memos"),
    create: (payload: Json) => this.request("POST", "/v1/memos", payload),
    dismiss: (memoId: string) => this.request("POST", `/v1/memos/${encodeURIComponent(memoId)}:dismiss`),
  };

  readonly phones = {
    ring: (phoneId: string, payload?: Json) =>
      this.request("POST", `/v1/phones/${encodeURIComponent(phoneId)}:ring`, payload),
  };

  readonly audioMemory = {
    status: () => this.request("GET", "/v1/audio-memory/status", undefined, true),
    ready: () => this.request("GET", "/v1/audio-memory/health/ready", undefined, true),
    settings: (payload?: Json) =>
      payload === undefined
        ? this.request("GET", "/v1/audio-memory/settings", undefined, true)
        : this.request("PATCH", "/v1/audio-memory/settings", payload, true),
    episodes: () => this.request("GET", "/v1/audio-memory/episodes", undefined, true),
    notes: (payload?: Json) =>
      payload === undefined
        ? this.request("GET", "/v1/audio-memory/notes", undefined, true)
        : this.request("POST", "/v1/audio-memory/notes", payload, true),
    query: (payload: Json) => this.request("POST", "/v1/audio-memory:query", payload, true),
    forget: (payload: Json) => this.request("POST", "/v1/audio-memory:forget", payload, true),
    clear: () => this.request("POST", "/v1/audio-memory:clear", undefined, true),
  };

  readonly knowledgeBases = {
    create: (payload: Json) => this.request("POST", "/v1/knowledge-bases", payload),
    retrieve: (knowledgeBaseId: string) =>
      this.request("GET", `/v1/knowledge-bases/${encodeURIComponent(knowledgeBaseId)}`),
    delete: (knowledgeBaseId: string) =>
      this.request("DELETE", `/v1/knowledge-bases/${encodeURIComponent(knowledgeBaseId)}`),
    documents: Object.assign(
      (knowledgeBaseId: string, payload?: Json) =>
        payload === undefined
          ? this.request("GET", `/v1/knowledge-bases/${encodeURIComponent(knowledgeBaseId)}/documents`)
          : this.request("POST", `/v1/knowledge-bases/${encodeURIComponent(knowledgeBaseId)}/documents:text`, payload),
      {
        list: (knowledgeBaseId: string) =>
          this.request("GET", `/v1/knowledge-bases/${encodeURIComponent(knowledgeBaseId)}/documents`),
        text: (knowledgeBaseId: string, payload: Json) =>
          this.request("POST", `/v1/knowledge-bases/${encodeURIComponent(knowledgeBaseId)}/documents:text`, payload),
        file: (knowledgeBaseId: string, payload: Json) =>
          this.request("POST", `/v1/knowledge-bases/${encodeURIComponent(knowledgeBaseId)}/documents:file`, payload),
        image: (knowledgeBaseId: string, payload: Json) =>
          this.request("POST", `/v1/knowledge-bases/${encodeURIComponent(knowledgeBaseId)}/documents:image`, payload),
      },
    ) as DocumentsApi,
    query: (knowledgeBaseId: string, payload: Json) =>
      this.request("POST", `/v1/knowledge-bases/${encodeURIComponent(knowledgeBaseId)}:query`, payload),
  };

  readonly images = {
    jobs: {
      create: (payload: Json) => this.request("POST", "/v2/images/jobs", payload),
      retrieve: (jobId: string) => this.request("GET", `/v2/images/jobs/${encodeURIComponent(jobId)}`),
      result: (jobId: string) => this.request("GET", `/v2/images/jobs/${encodeURIComponent(jobId)}/result`),
      cancel: (jobId: string, payload?: Json) =>
        this.request("POST", `/v2/images/jobs/${encodeURIComponent(jobId)}:cancel`, payload),
    },
  };

  readonly recordings = {
    create: (payload: Json) => this.request("POST", "/v1/audio/recordings", payload),
    list: () => this.request("GET", "/v1/audio/recordings"),
    get: (recordingId: string) => this.request("GET", `/v1/audio/recordings/${encodeURIComponent(recordingId)}`),
    delete: (recordingId: string) => this.request("DELETE", `/v1/audio/recordings/${encodeURIComponent(recordingId)}`),
    append: (recordingId: string, payload: Json) =>
      this.request("POST", `/v1/audio/recordings/${encodeURIComponent(recordingId)}/audio`, payload),
    finish: (recordingId: string, payload?: Json) =>
      this.request("POST", `/v1/audio/recordings/${encodeURIComponent(recordingId)}:finish`, payload),
    reprocess: (recordingId: string, payload?: Json) =>
      this.request("POST", `/v1/audio/recordings/${encodeURIComponent(recordingId)}:reprocess`, payload),
  };
  readonly audioRecordings = this.recordings;

  async chat(model: string, messages: ChatMessage[]): Promise<string> {
    const body = await this.request<{ choices: { message: { content: string } }[] }>(
      "POST",
      "/v1/chat/completions",
      { model, messages },
    );
    return body.choices[0].message.content;
  }
}

export function connect(options: OlgpClientOptions = {}): OlgpClient {
  return new OlgpClient(options);
}
