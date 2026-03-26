"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";

import {
  type ActionResponse,
  clearAuthSession,
  getBackendUrl,
  loadAuthSession,
  type PortalHomeResponse,
  subjectPortalPath,
  type MeResponse,
  type SessionInfo,
  type SessionsResponse,
  type SubjectType,
} from "../lib/auth";
import {
  buildRegistrationRequest,
  describePasskeyError,
  isPasskeySupported,
  serializeRegistrationCredential,
  type PasskeyRegisterOptionsResponse,
  type PasskeyRegisterVerifyResponse,
} from "../lib/passkey";

type PortalClientProps = {
  expectedSubjectType: SubjectType;
  portalApiPath: string;
  title: string;
  description: string;
};

type PortalStateResponse = {
  me: MeResponse;
  sessions: SessionsResponse;
  portal: PortalHomeResponse;
};

type ApiErrorResponse = {
  error?: {
    message?: string;
  };
};

export function PortalClient({
  expectedSubjectType,
  portalApiPath,
  title,
  description,
}: PortalClientProps) {
  const router = useRouter();
  const [data, setData] = useState<MeResponse | null>(null);
  const [portal, setPortal] = useState<PortalHomeResponse | null>(null);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [passkeyMessage, setPasskeyMessage] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [actionKey, setActionKey] = useState<string | null>(null);

  function getAuthOrRedirect() {
    const auth = loadAuthSession();
    if (!auth) {
      clearAuthSession();
      router.replace("/login");
      return null;
    }

    return auth;
  }

  async function loadPortalState(
    auth: NonNullable<ReturnType<typeof loadAuthSession>>,
  ): Promise<PortalStateResponse | null> {
    const [meResponse, sessionsResponse, portalResponse] = await Promise.all([
      fetch(`${getBackendUrl()}/api/auth/me`, {
        headers: {
          Authorization: `Bearer ${auth.access_token}`,
        },
      }),
      fetch(`${getBackendUrl()}/api/auth/sessions`, {
        headers: {
          Authorization: `Bearer ${auth.access_token}`,
        },
      }),
      fetch(`${getBackendUrl()}${portalApiPath}`, {
        headers: {
          Authorization: `Bearer ${auth.access_token}`,
        },
      }),
    ]);

    if (
      meResponse.status === 401 ||
      sessionsResponse.status === 401 ||
      portalResponse.status === 401
    ) {
      clearAuthSession();
      router.replace("/login");
      return null;
    }

    if (!meResponse.ok || !sessionsResponse.ok || !portalResponse.ok) {
      const payload = (await portalResponse.json().catch(() => null)) as
        | ApiErrorResponse
        | null;
      throw new Error(
        payload?.error?.message ?? "无法获取当前主体、会话列表或 portal 数据。",
      );
    }

    return {
      me: (await meResponse.json()) as MeResponse,
      sessions: (await sessionsResponse.json()) as SessionsResponse,
      portal: (await portalResponse.json()) as PortalHomeResponse,
    };
  }

  function applyPortalState(payload: PortalStateResponse) {
    setData(payload.me);
    setSessions(payload.sessions.sessions);
    setPortal(payload.portal);
  }

  useEffect(() => {
    let cancelled = false;

    async function load() {
      const auth = getAuthOrRedirect();

      if (!auth) {
        return;
      }

      if (auth.subject.subject_type !== expectedSubjectType) {
        router.replace(subjectPortalPath(auth.subject.subject_type));
        return;
      }

      try {
        const payload = await loadPortalState(auth);
        if (!payload || cancelled) {
          return;
        }

        applyPortalState(payload);
      } catch (error) {
        if (cancelled) {
          return;
        }

        setError(
          error instanceof Error
            ? error.message
            : "无法获取当前主体、会话列表或 portal 数据。",
        );
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    }

    void load();

    return () => {
      cancelled = true;
    };
  }, [expectedSubjectType, portalApiPath, router]);

  async function runAction(
    key: string,
    action: () => Promise<Response>,
    onSuccess: (payload: ActionResponse) => Promise<void> | void,
  ) {
    setActionKey(key);
    setError(null);
    setPasskeyMessage(null);

    try {
      const response = await action();

      if (response.status === 401) {
        clearAuthSession();
        router.replace("/login");
        return;
      }

      const payload = (await response.json().catch(() => null)) as
        | ActionResponse
        | {
            error?: {
              message?: string;
            };
          }
        | null;

      if (!response.ok) {
        setError(payload && "error" in payload ? payload.error?.message ?? "操作失败。" : "操作失败。");
        return;
      }

      await onSuccess(payload as ActionResponse);
    } finally {
      setActionKey(null);
    }
  }

  async function reloadSessions() {
    const auth = getAuthOrRedirect();
    if (!auth) {
      return;
    }
    const payload = await loadPortalState(auth);
    if (!payload) {
      return;
    }

    applyPortalState(payload);
  }

  async function registerPasskey() {
    const auth = getAuthOrRedirect();
    if (!auth) {
      return;
    }

    if (!isPasskeySupported()) {
      setError("当前浏览器或上下文不支持 Passkey，无法完成绑定。");
      return;
    }

    setActionKey("passkey-register");
    setError(null);
    setPasskeyMessage(null);

    try {
      const optionsResponse = await fetch(
        `${getBackendUrl()}/api/auth/passkey/register/options`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${auth.access_token}`,
          },
          body: JSON.stringify({}),
        },
      );

      if (!optionsResponse.ok) {
        const payload = (await optionsResponse.json().catch(() => null)) as
          | {
              error?: {
                message?: string;
              };
            }
          | null;
        setError(payload?.error?.message ?? "无法获取 Passkey 绑定挑战。");
        return;
      }

      const optionsPayload =
        (await optionsResponse.json()) as PasskeyRegisterOptionsResponse;
      const credential = (await navigator.credentials.create({
        publicKey: buildRegistrationRequest(optionsPayload.public_key),
      })) as PublicKeyCredential | null;

      if (!credential) {
        setError("浏览器没有返回可绑定的 Passkey。");
        return;
      }

      const verifyResponse = await fetch(
        `${getBackendUrl()}/api/auth/passkey/register/verify`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${auth.access_token}`,
          },
          body: JSON.stringify({
            challenge_id: optionsPayload.challenge_id,
            credential: serializeRegistrationCredential(credential),
          }),
        },
      );

      if (!verifyResponse.ok) {
        const payload = (await verifyResponse.json().catch(() => null)) as
          | {
              error?: {
                message?: string;
              };
            }
          | null;
        setError(payload?.error?.message ?? "Passkey 绑定失败。");
        return;
      }

      const payload =
        (await verifyResponse.json()) as PasskeyRegisterVerifyResponse;
      setPasskeyMessage(
        `${payload.message}：${payload.authenticator_label} (${payload.credential_id.slice(0, 16)}...)`,
      );
    } catch (error) {
      setError(describePasskeyError(error));
    } finally {
      setActionKey(null);
    }
  }

  return (
    <main className="mx-auto flex min-h-screen max-w-5xl flex-col justify-center px-6 py-16">
      <section className="rounded-[32px] border border-slate-200/70 bg-white/90 p-8 shadow-[0_20px_80px_rgba(15,23,42,0.08)] backdrop-blur">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <p className="text-sm font-semibold uppercase tracking-[0.3em] text-sky-700">
              Portal
            </p>
            <h1 className="mt-4 text-4xl font-semibold tracking-tight text-slate-950">
              {title}
            </h1>
            <p className="mt-4 max-w-2xl text-base leading-7 text-slate-600">
              {description}
            </p>
          </div>
          <Link
            className="rounded-2xl border border-slate-200 px-4 py-2 text-sm font-medium text-slate-700 transition hover:border-slate-300 hover:bg-slate-50"
            href="/login"
            onClick={() => clearAuthSession()}
          >
            返回登录
          </Link>
        </div>

        <div className="mt-8 flex flex-wrap gap-3">
          <button
            className="rounded-2xl bg-slate-950 px-4 py-3 text-sm font-medium text-white transition hover:bg-slate-800 disabled:cursor-not-allowed disabled:bg-slate-400"
            disabled={actionKey === "logout"}
            onClick={() =>
              runAction(
                "logout",
                async () => {
                  const auth = loadAuthSession();
                  return fetch(`${getBackendUrl()}/api/auth/logout`, {
                    method: "POST",
                    headers: {
                      Authorization: `Bearer ${auth?.access_token ?? ""}`,
                    },
                  });
                },
                () => {
                  clearAuthSession();
                  router.replace("/login");
                },
              )
            }
            type="button"
          >
            退出当前设备
          </button>
          <button
            className="rounded-2xl border border-sky-200 bg-sky-50 px-4 py-3 text-sm font-medium text-sky-700 transition hover:border-sky-300 hover:bg-sky-100 disabled:cursor-not-allowed disabled:opacity-60"
            disabled={actionKey === "passkey-register"}
            onClick={registerPasskey}
            type="button"
          >
            绑定 Passkey
          </button>
          <button
            className="rounded-2xl border border-red-200 bg-red-50 px-4 py-3 text-sm font-medium text-red-700 transition hover:border-red-300 hover:bg-red-100 disabled:cursor-not-allowed disabled:opacity-60"
            disabled={actionKey === "logout-all"}
            onClick={() =>
              runAction(
                "logout-all",
                async () => {
                  const auth = loadAuthSession();
                  return fetch(`${getBackendUrl()}/api/auth/logout-all`, {
                    method: "POST",
                    headers: {
                      Authorization: `Bearer ${auth?.access_token ?? ""}`,
                    },
                  });
                },
                () => {
                  clearAuthSession();
                  router.replace("/login");
                },
              )
            }
            type="button"
          >
            全部登出
          </button>
        </div>

        {isLoading ? (
          <div className="mt-8 rounded-2xl border border-slate-200 bg-slate-50 px-4 py-4 text-sm text-slate-600">
            正在校验 access token 并加载当前主体与会话列表...
          </div>
        ) : null}

        {error ? (
          <div className="mt-8 rounded-2xl border border-red-200 bg-red-50 px-4 py-4 text-sm text-red-700">
            {error}
          </div>
        ) : null}

        {passkeyMessage ? (
          <div className="mt-8 rounded-2xl border border-sky-200 bg-sky-50 px-4 py-4 text-sm text-sky-700">
            {passkeyMessage}
          </div>
        ) : null}

        {data ? (
          <div className="mt-8 grid gap-4 md:grid-cols-2">
            <InfoCard label="Subject ID" value={data.subject.id} />
            <InfoCard label="Subject Type" value={data.subject.subject_type} />
            <InfoCard label="Display Name" value={data.subject.display_name} />
            <InfoCard label="Status" value={data.subject.status} />
            <InfoCard label="Session ID" value={data.session.session_id} />
            <InfoCard label="Device ID" value={data.session.device_id} />
          </div>
        ) : null}

        {portal ? (
          <div className="mt-10 rounded-3xl border border-slate-200 bg-slate-50/80 p-6">
            <p className="text-sm font-semibold uppercase tracking-[0.3em] text-sky-700">
              Portal API
            </p>
            <h2 className="mt-3 text-2xl font-semibold tracking-tight text-slate-950">
              {portal.portal_title}
            </h2>
            <p className="mt-4 max-w-3xl text-base leading-7 text-slate-600">
              {portal.summary}
            </p>

            <div className="mt-6 grid gap-4 md:grid-cols-3">
              {portal.highlights.map((highlight) => (
                <div
                  key={highlight.label}
                  className="rounded-2xl border border-slate-200 bg-white px-4 py-4"
                >
                  <p className="text-xs font-semibold uppercase tracking-[0.2em] text-slate-500">
                    {highlight.label}
                  </p>
                  <p className="mt-3 text-base font-semibold text-slate-950">
                    {highlight.value}
                  </p>
                  <p className="mt-2 text-sm leading-6 text-slate-600">
                    {highlight.note}
                  </p>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        <div className="mt-10 rounded-3xl border border-slate-200 bg-slate-50/80 p-6">
          <p className="text-sm font-semibold uppercase tracking-[0.3em] text-sky-700">
            Passkey
          </p>
          <h2 className="mt-3 text-2xl font-semibold tracking-tight text-slate-950">
            绑定当前主体的 Passkey
          </h2>
          <p className="mt-4 max-w-3xl text-base leading-7 text-slate-600">
            当前阶段优先完成 Passkey 绑定。点击“绑定 Passkey”后会向后端申请 challenge，
            再调用浏览器原生 WebAuthn API，把 credential 元数据写入服务端 `passkey_credentials`。
          </p>
          <div className="mt-6 grid gap-4 md:grid-cols-3">
            <InfoCard
              label="Binding Scope"
              value="Current Subject"
            />
            <InfoCard
              label="Challenge Storage"
              value="Moka Cache"
            />
            <InfoCard
              label="Fallback"
              value={isPasskeySupported() ? "Browser Ready" : "Use Password / OTP"}
            />
          </div>
        </div>

        <div className="mt-10">
          <div className="flex items-center justify-between gap-4">
            <div>
              <p className="text-sm font-semibold uppercase tracking-[0.3em] text-sky-700">
                Sessions
              </p>
              <h2 className="mt-3 text-2xl font-semibold tracking-tight text-slate-950">
                多设备会话列表
              </h2>
            </div>
            <span className="rounded-full bg-slate-100 px-3 py-1 text-xs font-medium text-slate-600">
              {sessions.length} sessions
            </span>
          </div>

          <div className="mt-6 grid gap-4">
            {sessions.map((session) => (
              <article
                key={session.session_id}
                className="rounded-3xl border border-slate-200 bg-slate-50/80 p-5"
              >
                <div className="flex flex-wrap items-start justify-between gap-4">
                  <div>
                    <div className="flex flex-wrap items-center gap-2">
                      <h3 className="text-lg font-semibold text-slate-950">
                        {session.device_label}
                      </h3>
                      <Badge tone={session.is_current ? "sky" : "slate"}>
                        {session.is_current ? "Current" : "Other"}
                      </Badge>
                      <Badge tone={session.status === "ACTIVE" ? "green" : "slate"}>
                        {session.status}
                      </Badge>
                    </div>
                    <p className="mt-2 break-all text-sm text-slate-600">
                      {session.user_agent}
                    </p>
                  </div>

                  <button
                    className="rounded-2xl border border-slate-200 px-4 py-2 text-sm font-medium text-slate-700 transition hover:border-slate-300 hover:bg-white disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={
                      session.status !== "ACTIVE" || actionKey === session.session_id
                    }
                    onClick={() =>
                      runAction(
                        session.session_id,
                        async () => {
                          const auth = loadAuthSession();
                          return fetch(
                            `${getBackendUrl()}/api/auth/sessions/${session.session_id}`,
                            {
                              method: "DELETE",
                              headers: {
                                Authorization: `Bearer ${auth?.access_token ?? ""}`,
                              },
                            },
                          );
                        },
                        async () => {
                          if (session.is_current) {
                            clearAuthSession();
                            router.replace("/login");
                            return;
                          }

                          await reloadSessions();
                        },
                      )
                    }
                    type="button"
                  >
                    {session.is_current ? "撤销当前会话" : "撤销该会话"}
                  </button>
                </div>

                <div className="mt-5 grid gap-3 text-sm text-slate-600 md:grid-cols-2 lg:grid-cols-4">
                  <SessionMeta label="Session ID" value={session.session_id} />
                  <SessionMeta label="Device ID" value={session.device_id} />
                  <SessionMeta label="Created At" value={formatDateTime(session.created_at)} />
                  <SessionMeta label="Last Seen" value={formatDateTime(session.last_seen_at)} />
                  <SessionMeta label="Expires At" value={formatDateTime(session.expires_at)} />
                  <SessionMeta label="Login Method" value={session.login_method} />
                </div>
              </article>
            ))}
          </div>
        </div>
      </section>
    </main>
  );
}

function InfoCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-2xl border border-slate-200 bg-slate-50/80 px-4 py-4">
      <p className="text-xs font-semibold uppercase tracking-[0.2em] text-slate-500">
        {label}
      </p>
      <p className="mt-3 break-all text-base font-medium text-slate-900">{value}</p>
    </div>
  );
}

function SessionMeta({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-[11px] font-semibold uppercase tracking-[0.2em] text-slate-500">
        {label}
      </p>
      <p className="mt-2 break-all text-sm text-slate-700">{value}</p>
    </div>
  );
}

function Badge({
  children,
  tone,
}: {
  children: React.ReactNode;
  tone: "sky" | "green" | "slate";
}) {
  const className =
    tone === "sky"
      ? "bg-sky-100 text-sky-700"
      : tone === "green"
        ? "bg-emerald-100 text-emerald-700"
        : "bg-slate-200 text-slate-700";

  return (
    <span className={`rounded-full px-3 py-1 text-xs font-medium ${className}`}>
      {children}
    </span>
  );
}

function formatDateTime(value: string) {
  const date = new Date(value);

  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return date.toLocaleString("zh-CN", {
    hour12: false,
  });
}
