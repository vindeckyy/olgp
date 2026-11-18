type Json = Record<string, unknown>;

export type EngineSessionCreateInput = {
  model: string;
  task: string;
};

export interface EngineSession {
  id: string;
  model: string;
  task: string;
  state: "ready";
}

type DocumentsApi = {
  (knowledgeBaseId: string, payload?: Json): Promise<unknown>;
  list(knowledgeBaseId: string): Promise<unknown>;
  text(knowledgeBaseId: string, payload: Json): Promise<unknown>;
  file(knowledgeBaseId: string, payload: Json): Promise<unknown>;
  image(knowledgeBaseId: string, payload: Json): Promise<unknown>;
};

export type RecordingPcm = string | Uint8Array | ArrayBuffer | Json;

export interface RecordingCapture {
  id: string;
  recording: Json;
  append(pcm: RecordingPcm, seq?: number): Promise<unknown>;
  appendResumable(pcm: RecordingPcm): Promise<unknown>;
  finish(options?: Json): Promise<unknown>;
  reprocess(options?: Json): Promise<unknown>;
  retrieve(): Promise<unknown>;
  delete(): Promise<unknown>;
}

function unwrapResult(body: unknown): Json {
  if (body && typeof body === "object" && "result" in body) {
    const result = body.result;
    if (result && typeof result === "object") {
      return result as Json;
    }
  }
  if (body && typeof body === "object") {
    return body as Json;
  }
  throw new Error("olgp: recording response is not an object");
}

function recordingFrom(body: unknown): { recording: Json; id: string; sequence: number } {
  const recording = unwrapResult(body);
  const id = recording.id;
  if (typeof id !== "string") {
    throw new Error("olgp: recording response has no id");
  }
  const count = recording.chunk_count ?? recording.chunkCount;
  return { recording, id, sequence: typeof count === "number" ? count : 0 };
}

function pcmRequest(pcm: RecordingPcm, seq: number): Json {
  if (pcm instanceof ArrayBuffer) {
    return pcmRequest(new Uint8Array(pcm), seq);
  }
  if (pcm instanceof Uint8Array) {
    let binary = "";
    for (const byte of pcm) {
      binary += String.fromCharCode(byte);
    }
    return { seq, pcm: btoa(binary) };
  }
  if (typeof pcm === "string") {
    return { seq, pcm };
  }
  return { seq, ...pcm };
}

export interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}
export type RealtimeMode = "voice" | "chat";
export type RealtimeEvent = Json & { type: string };
export type RealtimeConnectOptions = { mode: RealtimeMode };

export class RealtimeSession {
  private readonly socket: WebSocket;
  private readonly eventListeners = new Set<(event: RealtimeEvent) => void>();
  private readonly audioListeners = new Set<(pcm: Uint8Array) => void>();
  private readonly queue: (string | Uint8Array)[] = [];

  constructor(baseUrl: string, token: string, mode: RealtimeMode) {
    const url = `${baseUrl.replace(/^http/, "ws")}/v1/realtime?mode=${encodeURIComponent(mode)}&token=${encodeURIComponent(token)}`;
    this.socket = new WebSocket(url);
    this.socket.binaryType = "arraybuffer";
    this.socket.onopen = () => {
      for (const frame of this.queue.splice(0)) {
        this.socket.send(frame);
      }
    };
    this.socket.onmessage = (message) => {
      const data = message.data;
      if (typeof data === "string") {
        let event: unknown;
        try {
          event = JSON.parse(data);
        } catch {
          return;
        }
        if (event && typeof event === "object" && "type" in event && typeof event.type === "string") {
          const parsed = event as RealtimeEvent;
          for (const listener of [...this.eventListeners]) {
            listener(parsed);
          }
        }
      } else if (data instanceof ArrayBuffer || data instanceof Uint8Array) {
        const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
        const pcm = bytes[0] === 0x01 ? bytes.subarray(1) : bytes;
        for (const listener of [...this.audioListeners]) {
          listener(pcm);
        }
      }
    };
  }

  // String PCM is base64, matching the recordings API.
  sendPcm(pcm: Uint8Array | string): void {
    const bytes = typeof pcm === "string" ? RealtimeSession.decodePcm(pcm) : pcm;
    const frame = new Uint8Array(bytes.length + 1);
    frame[0] = 0x01;
    frame.set(bytes, 1);
    this.send(frame);
  }

  sendText(text: string): void {
    this.control("text_input", { text });
  }

  commit(): void {
    this.control("commit");
  }

  cancel(): void {
    this.control("cancel");
  }

  reset(): void {
    this.control("reset");
  }

  onEvent(listener: (event: RealtimeEvent) => void): () => void {
    this.eventListeners.add(listener);
    return () => this.eventListeners.delete(listener);
  }

  onAudio(listener: (pcm: Uint8Array) => void): () => void {
    this.audioListeners.add(listener);
    return () => this.audioListeners.delete(listener);
  }

  close(): void {
    this.eventListeners.clear();
    this.audioListeners.clear();
    this.queue.length = 0;
    if (this.socket.readyState === 0 || this.socket.readyState === 1) {
      this.socket.close();
    }
  }

  private control(type: string, extra: Json = {}): void {
    this.send(JSON.stringify({ type, ...extra }));
  }

  private send(frame: string | Uint8Array): void {
    if (this.socket.readyState === 1) {
      this.socket.send(frame);
    } else if (this.socket.readyState === 0) {
      this.queue.push(frame);
    } else {
      throw new Error("olgp: realtime session is closed");
    }
  }

  private static decodePcm(pcm: string): Uint8Array {
    const binary = atob(pcm);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }
}

export interface OlgpClientOptions {
  baseUrl?: string;
  token?: string;
  memoryToken?: string;
  searchProvider?: string;
  tavilyKey?: string;
}

export interface ImageJobPollOptions {
  intervalMs?: number;
  timeoutMs?: number;
}

export type ImageResultBytes = {
  data: Uint8Array;
  contentType: string;
};

const IMAGE_JOB_TERMINAL_STATES: Record<string, true> = {
  succeeded: true,
  failed: true,
  cancelled: true,
  model_missing: true,
};
const IMAGE_JOB_FAILED_STATES: Record<string, true> = {
  failed: true,
  model_missing: true,
};

export class OlgpClient {
  private readonly baseUrl: string;
  private readonly token: string;
  private readonly memoryToken: string;
  private readonly searchProvider: string;
  private readonly tavilyKey: string;

  constructor(options: OlgpClientOptions = {}) {
    this.baseUrl = (options.baseUrl ?? process.env.OLGP_BASE_URL ?? "http://olgp.local:8787").replace(/\/$/, "");
    this.token = options.token ?? process.env.OLGP_DEVICE_TOKEN ?? "";
    this.memoryToken = options.memoryToken ?? process.env.OLGP_MEMORY_TOKEN ?? "";
    this.searchProvider = options.searchProvider ?? process.env.OLGP_SEARCH_PROVIDER ?? "";
    this.tavilyKey = options.tavilyKey ?? process.env.OLGP_TAVILY_KEY ?? "";
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
    if (this.searchProvider) {
      headers["X-Wici-Search-Provider"] = this.searchProvider;
    }
    if (this.tavilyKey) {
      headers["X-Wici-Tavily-Key"] = this.tavilyKey;
    }
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

  private async requestBytes(path: string): Promise<ImageResultBytes> {
    const response = await fetch(`${this.baseUrl}${path}`, {
      headers: { Authorization: `Bearer ${this.token}` },
    });
    if (!response.ok) {
      let detail = "";
      try {
        const body = (await response.json()) as { error?: unknown };
        detail =
          typeof body.error === "string"
            ? body.error
            : body.error && typeof body.error === "object" && "message" in body.error
              ? String(body.error.message)
              : "";
      } catch {
        // Non-JSON error bodies still surface as HTTP errors.
      }
      throw new Error(`olgp: HTTP ${response.status}${detail ? `: ${detail}` : ""}`);
    }
    return {
      data: new Uint8Array(await response.arrayBuffer()),
      contentType: response.headers.get("content-type") ?? "application/octet-stream",
    };
  }

  private async *imageJobSnapshots(
    jobId: string,
    options: ImageJobPollOptions = {},
  ): AsyncGenerator<Json> {
    const intervalMs = options.intervalMs ?? 250;
    const timeoutMs = options.timeoutMs;
    const deadlineMs = timeoutMs === undefined ? Number.POSITIVE_INFINITY : Date.now() + timeoutMs;
    for (;;) {
      const job = (await this.request(
        "GET",
        `/v2/images/jobs/${encodeURIComponent(jobId)}`,
      )) as Json;
      yield job;
      if (typeof job.status === "string" && IMAGE_JOB_TERMINAL_STATES[job.status] === true) {
        return;
      }
      if (Date.now() >= deadlineMs) {
        throw new Error(`olgp: image job ${jobId} timed out after ${timeoutMs}ms`);
      }
      const delay = Promise.withResolvers<void>();
      setTimeout(delay.resolve, intervalMs);
      await delay.promise;
    }
  }

  private recordingCapture(body: unknown): RecordingCapture {
    const initial = recordingFrom(body);
    const id = initial.id;
    let sequence = initial.sequence;
    const capture: RecordingCapture = {
      id,
      recording: initial.recording,
      append: async (pcm, seq = sequence) => {
        const ack = await this.recordings.append(id, pcmRequest(pcm, seq));
        sequence = seq + 1;
        return ack;
      },
      appendResumable: async (pcm) => {
        try {
          return await capture.append(pcm);
        } catch {
          const resumed = recordingFrom(await this.recordings.get(id));
          sequence = resumed.sequence;
          capture.recording = resumed.recording;
          return capture.append(pcm);
        }
      },
      finish: async (options) => unwrapResult(await this.recordings.finish(id, options)),
      reprocess: async (options) => unwrapResult(await this.recordings.reprocess(id, options)),
      retrieve: async () => {
        const current = recordingFrom(await this.recordings.get(id));
        sequence = current.sequence;
        capture.recording = current.recording;
        return current.recording;
      },
      delete: () => this.recordings.delete(id),
    };
    return capture;
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

  readonly engine = {
    status: () => this.request("GET", "/engine/status"),
    capabilities: () => this.request("GET", "/engine/capabilities"),
    sessions: Object.assign(
      () => this.request("GET", "/engine/sessions"),
      {
        create: (input: EngineSessionCreateInput) =>
          this.request<EngineSession>("POST", "/engine/sessions", input),
      },
    ),
  };

  readonly images = {
    jobs: {
      create: (payload: Json) => this.request("POST", "/v2/images/jobs", payload),
      retrieve: (jobId: string) => this.request("GET", `/v2/images/jobs/${encodeURIComponent(jobId)}`),
      observe: (jobId: string, options: ImageJobPollOptions = {}) =>
        this.imageJobSnapshots(jobId, options),
      wait: async (jobId: string, options: ImageJobPollOptions = {}) => {
        for await (const job of this.imageJobSnapshots(jobId, options)) {
          const status = job.status;
          if (typeof status === "string" && IMAGE_JOB_FAILED_STATES[status] === true) {
            throw new Error(`olgp: image job ${jobId} ${status}`);
          }
          if (typeof status === "string" && IMAGE_JOB_TERMINAL_STATES[status] === true) {
            return job;
          }
        }
        throw new Error(`olgp: image job ${jobId} produced no state`);
      },
      result: (jobId: string) => this.request("GET", `/v2/images/jobs/${encodeURIComponent(jobId)}/result`),
      resultBytes: (jobId: string) =>
        this.requestBytes(`/v2/images/jobs/${encodeURIComponent(jobId)}/result`),
      cancel: (jobId: string, payload?: Json) =>
        this.request("POST", `/v2/images/jobs/${encodeURIComponent(jobId)}:cancel`, payload),
    },
    framesProcess: (payload: Json) => this.request("POST", "/v2/images/frames:process", payload),
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
    start: async (input?: Json) =>
      this.recordingCapture(await this.request("POST", "/v1/audio/recordings", input ?? {})),
    resume: async (recordingId: string) =>
      this.recordingCapture(
        await this.request("GET", `/v1/audio/recordings/${encodeURIComponent(recordingId)}`),
      ),
  };
  readonly audioRecordings = this.recordings;
  readonly audio = { recordings: this.recordings };
  readonly realtime = {
    connect: (options: RealtimeConnectOptions): RealtimeSession =>
      new RealtimeSession(this.baseUrl, this.token, options.mode),
  };

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

export async function saveWiciImage(
  result: ImageResultBytes,
  path: string,
): Promise<Uint8Array> {
  if (typeof process !== "undefined" && process.versions?.node) {
    // Static import would break browser bundlers where node:fs/promises is absent.
    const { writeFile } = await import("node:fs/promises");
    await writeFile(path, result.data);
  }
  return result.data;
}
