export interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

export interface OlgpClientOptions {
  baseUrl?: string;
  token?: string;
}

export class OlgpClient {
  private readonly baseUrl: string;
  private readonly token: string;

  constructor(options: OlgpClientOptions = {}) {
    this.baseUrl = (options.baseUrl ?? process.env.OLGP_BASE_URL ?? "http://olgp.local:8787").replace(/\/$/, "");
    this.token = options.token ?? process.env.OLGP_DEVICE_TOKEN ?? "";
  }

  async chat(model: string, messages: ChatMessage[]): Promise<string> {
    const response = await fetch(`${this.baseUrl}/v1/chat/completions`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${this.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ model, messages }),
    });
    if (!response.ok) {
      throw new Error(`olgp: HTTP ${response.status}`);
    }
    const body = (await response.json()) as { choices: { message: { content: string } }[] };
    return body.choices[0].message.content;
  }
}

export function connect(options: OlgpClientOptions = {}): OlgpClient {
  return new OlgpClient(options);
}
