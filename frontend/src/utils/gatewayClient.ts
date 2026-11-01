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
  consensus_backing?: string;
  quorum_backed?: boolean;
  active_region: string;
  monotonic_clock_ms: number;
  lease_ttl_ms?: number;
  renewal_interval_ms?: number;
  safety_margin_ms?: number;
  active_leases_count?: number;
  total_quarantined_records?: number;
  ownerships: Array<{
    topic: string;
    partition: number;
    owner_region: string;
    epoch: number;
    active_lease: boolean;
    lease_expiry_ns?: number;
    remaining_lease_ms?: number;
  }>;
}

export interface OutboxTotals {
  byte_size: number;
  pending_count: number;
  oldest_entry_age_ms: number | null;
}

export interface StreamsResponse {
  streams: StreamTelemetry[];
  count: number;
  by_peer?: Record<string, OutboxTotals>;
  outbox_totals?: OutboxTotals;
}

export interface AuditVerificationResult {
  verification_result: string;
  hash_chain_valid?: boolean;
  tamper_detected?: boolean;
  zero_tamper_detected?: boolean;
  total_entries?: number;
  anchored_boundary: boolean | number;
  unanchored_tail_count?: number;
  total_streams_audited: number;
  total_checkpoints_verified: number;
  cryptographic_signatures_valid: boolean;
}

export interface ScenarioResult {
  scenario: string;
  status: string;
  details: string;
}

export interface ConsumerGroupsResponse {
  status: string;
  consumer_groups: Array<{
    group_id: string;
    generation_id: number;
    protocol_type: string;
    leader_id: string | null;
    state: string;
    members: Record<string, {
      member_id: string;
      client_id: string;
      client_host: string;
      session_timeout_ms: number;
      last_heartbeat_ms: number;
      subscribed_topics: string[];
      assigned_partitions: Record<string, number[]>;
    }>;
  }>;
  count: number;
}

export interface RetentionStatusResponse {
  status: string;
  active_rules: number;
  reclaimed_bytes_total: number;
  reclaimed_records_total: number;
  last_sweep_timestamp_ms: number;
}

export interface QuorumStatusResponse {
  configured: boolean;
  quorum_size: number;
  ack_level: string;
  peers: Array<{
    node_id: number;
    region: string;
    endpoint: string;
    is_voter: boolean;
    connected: boolean;
    last_ack_ms: number;
  }>;
  commit_indices: Record<string, number>;
}

export type GatewayTopologyMode = "same-origin-proxy" | "separate-origin" | "local-dev";

export function resolveGatewayConfig(): { baseUrl: string; configError: string | null; topologyMode: GatewayTopologyMode } {
  // Check for explicit Vite environment configuration
  const envUrl = typeof import.meta !== "undefined" && import.meta.env?.VITE_KAMISH_GATEWAY_URL
    ? String(import.meta.env.VITE_KAMISH_GATEWAY_URL).trim()
    : "";

  if (envUrl) {
    // Parse the complete URL rather than trusting a scheme-only regular expression.
    try {
      const parsed = new URL(envUrl);
      if (
        (parsed.protocol !== "http:" && parsed.protocol !== "https:") ||
        !parsed.hostname ||
        parsed.username ||
        parsed.password ||
        parsed.search ||
        parsed.hash
      ) {
        throw new Error("URL must use HTTP(S), include a host, and contain no credentials, query, or fragment");
      }
      const pathPrefix = parsed.pathname.replace(/\/+$/, "");
      return {
        baseUrl: `${parsed.origin}${pathPrefix === "/" ? "" : pathPrefix}`,
        configError: null,
        topologyMode: "separate-origin",
      };
    } catch (error) {
      const detail = error instanceof Error ? error.message : "invalid URL";
      return {
        baseUrl: "",
        configError: `Invalid VITE_KAMISH_GATEWAY_URL '${envUrl}': ${detail}`,
        topologyMode: "separate-origin",
      };
    }
  }

  // Development fallback: when Vite dev server is on port 5173, point directly to standard gateway port 3001
  const isDev = typeof import.meta !== "undefined" && Boolean(import.meta.env?.DEV);
  const isPort5173 = typeof window !== "undefined" && window.location.port === "5173";
  if (isDev && isPort5173) {
    return {
      baseUrl: "http://127.0.0.1:3001",
      configError: null,
      topologyMode: "local-dev",
    };
  }

  // Production default: same-origin reverse-proxy deployment (e.g. nginx forwarding /api and /metrics)
  return {
    baseUrl: "",
    configError: null,
    topologyMode: "same-origin-proxy",
  };
}

class GatewayClient {
  private baseUrl: string;
  private configError: string | null = null;
  private topologyMode: GatewayTopologyMode = "same-origin-proxy";
  private csrfToken: string | null = null;
  private sessionToken: string | null = null;
  private user: GatewayUser | null = null;
  private sessionExpiredListeners: Array<() => void> = [];

  constructor(customBaseUrl?: string) {
    if (customBaseUrl !== undefined) {
      this.baseUrl = customBaseUrl.replace(/\/+$/, "");
      this.topologyMode = customBaseUrl ? "separate-origin" : "same-origin-proxy";
      this.configError = null;
    } else {
      const resolved = resolveGatewayConfig();
      this.baseUrl = resolved.baseUrl;
      this.configError = resolved.configError;
      this.topologyMode = resolved.topologyMode;
    }

    // Hydrate tokens from sessionStorage if present
    this.csrfToken = typeof sessionStorage !== "undefined" ? sessionStorage.getItem("kamish_csrf_token") : null;
    this.sessionToken = typeof sessionStorage !== "undefined" ? sessionStorage.getItem("kamish_session_token") : null;
    const storedUser = typeof sessionStorage !== "undefined" ? sessionStorage.getItem("kamish_user") : null;
    if (storedUser) {
      try {
        this.user = JSON.parse(storedUser);
      } catch {
        this.user = null;
      }
    }
  }

  public getBaseUrl(): string {
    return this.baseUrl;
  }

  public getConfigError(): string | null {
    return this.configError;
  }

  public getTopologyMode(): GatewayTopologyMode {
    return this.topologyMode;
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
    if (typeof sessionStorage !== "undefined") {
      sessionStorage.removeItem("kamish_csrf_token");
      sessionStorage.removeItem("kamish_session_token");
      sessionStorage.removeItem("kamish_user");
    }
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

  private buildUrl(endpoint: string): string {
    const cleanEndpoint = endpoint.startsWith("/") ? endpoint : `/${endpoint}`;
    if (!this.baseUrl) {
      return cleanEndpoint;
    }
    return `${this.baseUrl}${cleanEndpoint}`;
  }

  private async request<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
    if (this.configError) {
      throw new Error(`Gateway Configuration Error: ${this.configError}`);
    }

    const url = this.buildUrl(endpoint);
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

  public async getStreams(): Promise<StreamsResponse> {
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

  // --- Quorum, Consumer Groups, Retention, & Reconciliation Endpoints ---

  public async getQuorum(): Promise<QuorumStatusResponse> {
    return this.request<QuorumStatusResponse>("/api/quorum");
  }

  public async getConsumerGroups(): Promise<ConsumerGroupsResponse> {
    return this.request<ConsumerGroupsResponse>("/api/consumer-groups");
  }

  public async getRetention(): Promise<RetentionStatusResponse> {
    return this.request<RetentionStatusResponse>("/api/retention");
  }

  public async triggerReconciliation(targetRegion?: string): Promise<{
    status: string;
    targets_reconciled: string[];
    gaps_discovered: number;
    records_scheduled_into_outbox: number;
  }> {
    return this.request<{
      status: string;
      targets_reconciled: string[];
      gaps_discovered: number;
      records_scheduled_into_outbox: number;
    }>("/api/reconciliation/run", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ target_region: targetRegion || null }),
    });
  }

  public async getMetrics(): Promise<string> {
    if (this.configError) {
      throw new Error(`Gateway Configuration Error: ${this.configError}`);
    }
    const url = this.buildUrl("/metrics");
    const res = await fetch(url, { credentials: "include" });
    if (!res.ok) {
      throw new Error(`Failed to fetch /metrics: HTTP ${res.status}`);
    }
    return res.text();
  }

  public subscribeEvents(onMessage: (data: string) => void, onError?: (err: any) => void): () => void {
    if (this.configError) {
      if (onError) onError(new Error(`Gateway Configuration Error: ${this.configError}`));
      return () => { };
    }

    const eventSource = new EventSource(this.buildUrl("/api/events"), {
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
