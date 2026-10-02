// Thin fetch wrapper: JSON in/out, session cookie, CSRF header on state-changing calls.

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public details?: unknown,
  ) {
    super(message);
  }
}

let csrfToken = "";

export function setCsrfToken(token: string) {
  csrfToken = token;
}

async function request<T>(method: string, path: string, body?: unknown, raw = false): Promise<T> {
  const headers: Record<string, string> = { Accept: "application/json" };
  let payload: BodyInit | undefined;
  if (body instanceof FormData || body instanceof Blob) {
    payload = body;
  } else if (body !== undefined) {
    headers["Content-Type"] = "application/json";
    payload = JSON.stringify(body);
  }
  if (method !== "GET" && csrfToken) headers["X-CSRF-Token"] = csrfToken;
  const res = await fetch(path, { method, headers, body: payload, credentials: "same-origin" });
  if (!res.ok) {
    let data: { error?: string; message?: string; details?: unknown } = {};
    try {
      data = await res.json();
    } catch {
      /* not JSON */
    }
    throw new ApiError(res.status, data.error ?? "http_" + res.status, data.message ?? res.statusText, data.details);
  }
  if (raw) return res as unknown as T;
  return (await res.json()) as T;
}

export const api = {
  get: <T>(path: string) => request<T>("GET", path),
  post: <T>(path: string, body?: unknown) => request<T>("POST", path, body ?? {}),
  put: <T>(path: string, body?: unknown) => request<T>("PUT", path, body ?? {}),
  patch: <T>(path: string, body?: unknown) => request<T>("PATCH", path, body ?? {}),
  del: <T>(path: string) => request<T>("DELETE", path),
  upload: <T>(path: string, body: FormData | Blob) => request<T>("POST", path, body),
  /** Downloads a file produced by an authenticated endpoint. */
  async download(path: string, filename: string) {
    const res = await request<Response>("GET", path, undefined, true);
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
  },
};
