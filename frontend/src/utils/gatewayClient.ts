/**
 * Kamish Dashboard Gateway Client (Phase 5).
 * Bridges the React dashboard to the authenticated Rust Gateway daemon.
 */

export interface GatewayUser {
  username: string;
  role: "admin" | "operator" | "auditor" | "producer" | "consumer";
}

export interface GatewayStatus {
  status: string;
  version: string;
  region: string;
  mode: "LIVE" | "DEMO";
  known_peers: string[];
  degraded_links: string[];
  max_frame_bytes: number;
  max_payload_bytes: number;
  max_connections: number;
  timestamp_utc: string;
}

export interface StreamTelemetry {
  topic: string;
  partition: number;
  replication_lags: Record<string, number>;
}

export interface AuthorityState {
  authority_type: string;
  active_region: string;
  monotonic_clock_ms: number;
  ownerships: Array<{
    topic: string;
    partition: number;
    owner_region: string;
    epoch: number;
    active_lease: boolean;
  }>;
}

export interface AuditVerificationResult {
  verification_result: string;
  anchored_boundary: boolean;
  total_streams_audited: number;
  total_checkpoints_verified: number;
  cryptographic_signatures_valid: boolean;
  zero_tamper_detected: boolean;
}

export interface ScenarioResult {
  scenario: string;
  status: string;
  details: string;
}

class GatewayClient {
  private baseUrl: string;
  private csrfToken: string | null = null;
  private sessionToken: string | null = null;
  private user: GatewayUser | null = null;
  private sessionExpiredListeners: Array<() => void> = [];

  constructor(baseUrl: string = "") {
    this.baseUrl = baseUrl || (window.location.port === "5173" ? "http://127.0.0.1:3001" : "");
    // Hydrate tokens from sessionStorage if present
    this.csrfToken = sessionStorage.getItem("kamish_csrf_token");
    this.sessionToken = sessionStorage.getItem("kamish_session_token");
    const storedUser = sessionStorage.getItem("kamish_user");
    if (storedUser) {
      try {
        this.user = JSON.parse(storedUser);
      } catch {
        this.user = null;
      }
    }
  }

  public onSessionExpired(listener: () => void): () => void {
    this.sessionExpiredListeners.push(listener);
    return () => {
      this.sessionExpiredListeners = this.sessionExpiredListeners.filter(l => l !== listener);
    };
  }

  private notifySessionExpired() {
    this.csrfToken = null;
    this.sessionToken = null;
    this.user = null;
    sessionStorage.removeItem("kamish_csrf_token");
    sessionStorage.removeItem("kamish_session_token");
    sessionStorage.removeItem("kamish_user");
    for (const listener of this.sessionExpiredListeners) {
      listener();
    }
  }

  public getCurrentUser(): GatewayUser | null {
    return this.user;
  }

  public isAuthenticated(): boolean {
    return Boolean(this.sessionToken);
  }

  private async request<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`;
    const headers: Record<string, string> = {
      ...(options.headers as Record<string, string> || {}),
    };

    if (this.sessionToken) {
      headers["Authorization"] = `Bearer ${this.sessionToken}`;
    }

    if (this.csrfToken && (options.method === "POST" || options.method === "PUT" || options.method === "DELETE")) {
      headers["x-csrf-token"] = this.csrfToken;
    }

    const response = await fetch(url, {
      ...options,
      credentials: "include",
      headers,
    });

    if (response.status === 401) {
      this.notifySessionExpired();
      throw new Error("Session expired or unauthorized: please log in");
    }

    if (!response.ok) {
      let errorMsg = `HTTP ${response.status}: ${response.statusText}`;
      try {
        const errorJson = await response.json();
        if (errorJson.error) {
          errorMsg = errorJson.error;
        }
      } catch {
        // ignore
      }
      throw new Error(errorMsg);
    }

    return response.json();
  }

  // --- Auth Endpoints ---

  public async login(username: string, password: string): Promise<{ user: GatewayUser; session_token: string; csrf_token: string }> {
    const res = await this.request<{
      session_token: string;
      csrf_token: string;
      user: GatewayUser;
    }>("/api/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username, password }),
    });

    this.csrfToken = res.csrf_token;
    this.sessionToken = res.session_token;
    this.user = res.user;
    sessionStorage.setItem("kamish_csrf_token", res.csrf_token);
    sessionStorage.setItem("kamish_session_token", res.session_token);
    sessionStorage.setItem("kamish_user", JSON.stringify(res.user));
    return res;
  }

  public async logout(): Promise<void> {
    try {
      await this.request("/api/auth/logout", { method: "POST" });
    } finally {
      this.notifySessionExpired();
    }
  }

  public async getMe(): Promise<{ authenticated: boolean; user: GatewayUser; csrf_token: string }> {
    const res = await this.request<{
      authenticated: boolean;
      user: GatewayUser;
      csrf_token: string;
      session_token: string;
    }>("/api/auth/me");
    this.user = res.user;
    this.csrfToken = res.csrf_token;
    sessionStorage.setItem("kamish_user", JSON.stringify(res.user));
    sessionStorage.setItem("kamish_csrf_token", res.csrf_token);
    return res;
  }

  // --- Real Telemetry Endpoints ---

  public async getStatus(): Promise<GatewayStatus> {
    return this.request<GatewayStatus>("/api/status");
  }

  public async getStreams(): Promise<{ streams: StreamTelemetry[]; count: number }> {
    return this.request<{ streams: StreamTelemetry[]; count: number }>("/api/streams");
  }

  public async getAuthority(): Promise<AuthorityState> {
    return this.request<AuthorityState>("/api/authority");
  }

  public async getPolicy(): Promise<{
    policy_version_floor: number;
    enforcement: string;
    registered_topics: string[];
  }> {
    return this.request("/api/policy");
  }

  public async publishPolicy(policy_version: number): Promise<{ status: string; new_policy_version_floor: number }> {
    return this.request("/api/policy/publish", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ policy_version }),
    });
  }

  public async getAudit(): Promise<{
    audit_ledger_status: string;
    checkpoints_count: number;
    checkpoint_anchors: Array<{ stream: string; tail_hash: string; record_count: number }>;
    verification: string;
  }> {
    return this.request("/api/audit");
  }

  public async verifyAudit(): Promise<AuditVerificationResult> {
    return this.request<AuditVerificationResult>("/api/audit/verify", {
      method: "POST",
    });
  }

  public async getCheckers(): Promise<{
    history_checker: Record<string, any>;
    canary_scanner: Record<string, any>;
  }> {
    return this.request("/api/checkers");
  }

  public async getRouting(): Promise<{
    routing_policy: string;
    active_routes: Array<{
      target_region: string;
      circuit_breaker: string;
      smoothed_ewma_latency_ms: number;
      status: string;
    }>;
  }> {
    return this.request("/api/routing");
  }

  // --- Operations ---

  public async getTopics(): Promise<{ topics: any[] }> {
    return this.request("/api/topics");
  }

  public async createTopic(topic: string, partition_count: number, class_id: string): Promise<any> {
    return this.request("/api/topics", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ topic, partition_count, class_id }),
    });
  }

  public async produce(topic: string, partition: number, class_id: string, payload: string): Promise<any> {
    return this.request("/api/produce", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ topic, partition, class_id, payload }),
    });
  }

  public async fetch(topic: string, partition: number, offset: number, max_records: number = 10, class_id?: string): Promise<any> {
    return this.request("/api/fetch", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ topic, partition, offset, max_records, class_id }),
    });
  }

  public async runScenario(scenario: "residency" | "slowlink" | "outage" | "quorum"): Promise<ScenarioResult> {
    return this.request<ScenarioResult>("/api/scenarios/run", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ scenario }),
    });
  }

  public async getMode(): Promise<{ mode: "LIVE" | "DEMO" }> {
    return this.request<{ mode: "LIVE" | "DEMO" }>("/api/mode");
  }

  public async setMode(mode: "LIVE" | "DEMO"): Promise<{ status: string; mode: "LIVE" | "DEMO" }> {
    return this.request<{ status: string; mode: "LIVE" | "DEMO" }>("/api/mode", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ mode }),
    });
  }

  public subscribeEvents(onMessage: (data: string) => void, onError?: (err: any) => void): () => void {
    const eventSource = new EventSource(`${this.baseUrl}/api/events`, {
      withCredentials: true,
    });

    eventSource.onmessage = (event) => {
      onMessage(event.data);
    };

    if (onError) {
      eventSource.onerror = (err) => {
        onError(err);
      };
    }

    return () => {
      eventSource.close();
    };
  }
}

export const gatewayClient = new GatewayClient();
export default gatewayClient;
