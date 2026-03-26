"use client";

import { FormEvent, useState, useTransition } from "react";
import { useRouter } from "next/navigation";

import {
  type LoginResponse,
  type OtpRequestResponse,
  type SubjectType,
  getBackendUrl,
  saveAuthSession,
  subjectPortalPath,
} from "../lib/auth";
import {
  buildAuthenticationRequest,
  describePasskeyError,
  isPasskeySupported,
  serializeAuthenticationCredential,
  type PasskeyLoginOptionsResponse,
} from "../lib/passkey";

type ApiErrorResponse = {
  error?: {
    code?: string;
    message?: string;
  };
};

const subjectOptions: Array<{ value: SubjectType; label: string }> = [
  { value: "MEMBER", label: "Member" },
  { value: "COMMUNITY_STAFF", label: "Community Staff" },
  { value: "PLATFORM_STAFF", label: "Platform Staff" },
];

type LoginMethod = "PASSWORD" | "OTP" | "PASSKEY";

export function LoginForm() {
  const router = useRouter();
  const [isPending, startTransition] = useTransition();
  const [loginMethod, setLoginMethod] = useState<LoginMethod>("PASSWORD");
  const [subjectType, setSubjectType] = useState<SubjectType>("MEMBER");
  const [identifier, setIdentifier] = useState("member@example.com");
  const [password, setPassword] = useState("Password123!");
  const [otpCode, setOtpCode] = useState("");
  const [challenge, setChallenge] = useState<OtpRequestResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hint, setHint] = useState<string | null>(null);

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setHint(null);

    const response = await fetch(`${getBackendUrl()}/api/auth/password/login`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        subject_type: subjectType,
        identifier,
        password,
      }),
    });

    if (!response.ok) {
      const payload = (await response.json().catch(() => null)) as
        | ApiErrorResponse
        | null;
      setError(payload?.error?.message ?? "登录失败，请检查输入后重试。");
      return;
    }

    const payload = (await response.json()) as LoginResponse;
    saveAuthSession(payload);

    startTransition(() => {
      router.push(subjectPortalPath(payload.subject.subject_type));
    });
  }

  async function requestOtpCode() {
    setError(null);
    setHint(null);

    const response = await fetch(`${getBackendUrl()}/api/auth/otp/request`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        subject_type: subjectType,
        identifier,
      }),
    });

    if (!response.ok) {
      const payload = (await response.json().catch(() => null)) as
        | ApiErrorResponse
        | null;
      setError(payload?.error?.message ?? "验证码请求失败，请检查输入后重试。");
      return;
    }

    const payload = (await response.json()) as OtpRequestResponse;
    setChallenge(payload);
    setHint(
      payload.dev_code
        ? `开发环境验证码：${payload.dev_code}，有效期至 ${formatDateTime(payload.expires_at)}`
        : `验证码已发送到 ${payload.masked_destination}`,
    );
  }

  async function verifyOtpCode() {
    if (!challenge) {
      setError("请先请求验证码。");
      return;
    }

    setError(null);
    setHint(null);

    const response = await fetch(`${getBackendUrl()}/api/auth/otp/verify`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        challenge_id: challenge.challenge_id,
        code: otpCode,
      }),
    });

    if (!response.ok) {
      const payload = (await response.json().catch(() => null)) as
        | ApiErrorResponse
        | null;
      setError(payload?.error?.message ?? "验证码校验失败，请重试。");
      return;
    }

    const payload = (await response.json()) as LoginResponse;
    saveAuthSession(payload);

    startTransition(() => {
      router.push(subjectPortalPath(payload.subject.subject_type));
    });
  }

  async function loginWithPasskey() {
    if (!isPasskeySupported()) {
      setError("当前浏览器或上下文不支持 Passkey，请改用 Password / OTP。");
      return;
    }

    setError(null);
    setHint(null);

    const optionsResponse = await fetch(
      `${getBackendUrl()}/api/auth/passkey/login/options`,
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          subject_type: subjectType,
          identifier,
        }),
      },
    );

    if (!optionsResponse.ok) {
      const payload = (await optionsResponse.json().catch(() => null)) as
        | ApiErrorResponse
        | null;
      setError(payload?.error?.message ?? "无法获取 Passkey 登录挑战。");
      return;
    }

    const optionsPayload =
      (await optionsResponse.json()) as PasskeyLoginOptionsResponse;

    try {
      const credential = (await navigator.credentials.get({
        publicKey: buildAuthenticationRequest(optionsPayload.public_key),
      })) as PublicKeyCredential | null;

      if (!credential) {
        setError("浏览器没有返回 Passkey 凭证。");
        return;
      }

      const verifyResponse = await fetch(
        `${getBackendUrl()}/api/auth/passkey/login/verify`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
          },
          body: JSON.stringify({
            challenge_id: optionsPayload.challenge_id,
            credential: serializeAuthenticationCredential(credential),
          }),
        },
      );

      if (!verifyResponse.ok) {
        const payload = (await verifyResponse.json().catch(() => null)) as
          | ApiErrorResponse
          | null;
        setError(payload?.error?.message ?? "Passkey 登录失败。");
        return;
      }

      const payload = (await verifyResponse.json()) as LoginResponse;
      saveAuthSession(payload);

      startTransition(() => {
        router.push(subjectPortalPath(payload.subject.subject_type));
      });
    } catch (error) {
      setError(describePasskeyError(error));
    }
  }

  function resetOtpState(nextIdentifier?: string) {
    setChallenge(null);
    setOtpCode("");
    setError(null);
    setHint(null);
    if (typeof nextIdentifier === "string") {
      setIdentifier(nextIdentifier);
    }
  }

  return (
    <section className="grid gap-8 rounded-[32px] border border-slate-200/70 bg-white/90 p-8 shadow-[0_20px_80px_rgba(15,23,42,0.08)] backdrop-blur lg:grid-cols-[1.2fr_0.8fr]">
      <div>
        <p className="text-sm font-semibold uppercase tracking-[0.3em] text-sky-700">
          Identity Access
        </p>
        <h1 className="mt-4 text-4xl font-semibold tracking-tight text-slate-950 md:text-5xl">
          Multi-Subject Access Console
        </h1>
        <p className="mt-5 max-w-2xl text-base leading-7 text-slate-600">
          当前系统支持 Password、OTP 和 Passkey 三条登录路径，并保留
          服务端 session、portal 边界和清晰的前端访问入口。
        </p>
      </div>

      <form
        className="grid gap-4"
        onSubmit={loginMethod === "PASSWORD" ? onSubmit : (event) => event.preventDefault()}
      >
        <div className="grid grid-cols-3 gap-3">
          {(["PASSWORD", "OTP", "PASSKEY"] as const).map((value) => (
            <button
              key={value}
              className={`rounded-2xl px-4 py-3 text-sm font-medium transition ${
                loginMethod === value
                  ? "bg-slate-950 text-white"
                  : "border border-slate-200 bg-slate-50 text-slate-700 hover:bg-slate-100"
              }`}
              onClick={() => {
                setLoginMethod(value);
                resetOtpState();
              }}
              type="button"
            >
              {value === "PASSWORD"
                ? "Password"
                : value === "OTP"
                  ? "OTP"
                  : "Passkey"}
            </button>
          ))}
        </div>

        <label className="grid gap-2 text-sm font-medium text-slate-700">
          Subject Type
          <select
            className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-base text-slate-900 outline-none transition focus:border-sky-500"
            value={subjectType}
            onChange={(event) => {
              setSubjectType(event.target.value as SubjectType);
              resetOtpState();
            }}
          >
            {subjectOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <label className="grid gap-2 text-sm font-medium text-slate-700">
          Identifier
          <input
            className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-base text-slate-900 outline-none transition focus:border-sky-500"
            value={identifier}
            onChange={(event) => resetOtpState(event.target.value)}
            placeholder={
              loginMethod === "PASSWORD"
                ? "email / phone / member_no / staff_no"
                : loginMethod === "OTP"
                  ? "email / phone"
                  : "email / member_no / staff_no"
            }
          />
        </label>

        {loginMethod === "PASSWORD" ? (
          <label className="grid gap-2 text-sm font-medium text-slate-700">
            Password
            <input
              className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-base text-slate-900 outline-none transition focus:border-sky-500"
              type="password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
            />
          </label>
        ) : loginMethod === "OTP" ? (
          <>
            <div className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-4 text-sm text-slate-600">
              OTP 仅使用已配置的可达通道。当前 seed 数据中：
              `MEMBER` 支持 email / phone，Staff 支持 email。
            </div>

            <div className="flex flex-wrap gap-3">
              <button
                className="rounded-2xl bg-slate-950 px-5 py-3 text-base font-medium text-white transition hover:bg-slate-800 disabled:cursor-not-allowed disabled:bg-slate-400"
                disabled={isPending}
                onClick={requestOtpCode}
                type="button"
              >
                {challenge ? "重新请求验证码" : "获取验证码"}
              </button>
              {challenge ? (
                <span className="rounded-2xl border border-slate-200 bg-white px-4 py-3 text-sm text-slate-600">
                  有效期至 {formatDateTime(challenge.expires_at)}
                </span>
              ) : null}
            </div>

            <label className="grid gap-2 text-sm font-medium text-slate-700">
              OTP Code
              <input
                className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-base text-slate-900 outline-none transition focus:border-sky-500"
                inputMode="numeric"
                maxLength={6}
                value={otpCode}
                onChange={(event) => setOtpCode(event.target.value)}
                placeholder="6-digit code"
              />
            </label>
          </>
        ) : (
          <div className="grid gap-3">
            <div className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-4 text-sm leading-6 text-slate-600">
              Passkey 登录会先根据 `subject_type + identifier` 请求 challenge，再调用
              浏览器原生 WebAuthn API。若当前设备还没有绑定 Passkey，请先登录进入
              portal 页面完成绑定。
            </div>
            {!isPasskeySupported() ? (
              <div className="rounded-2xl border border-amber-200 bg-amber-50 px-4 py-4 text-sm text-amber-700">
                当前浏览器或当前访问上下文不支持 Passkey，当前系统仍可使用 Password / OTP。
              </div>
            ) : null}
          </div>
        )}

        {hint ? (
          <div className="rounded-2xl border border-sky-200 bg-sky-50 px-4 py-3 text-sm text-sky-700">
            {hint}
          </div>
        ) : null}

        {error ? (
          <div className="rounded-2xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
            {error}
          </div>
        ) : null}

        <button
          className="rounded-2xl bg-slate-950 px-5 py-3 text-base font-medium text-white transition hover:bg-slate-800 disabled:cursor-not-allowed disabled:bg-slate-400"
          disabled={isPending || (loginMethod === "OTP" && !challenge)}
          onClick={
            loginMethod === "OTP"
              ? verifyOtpCode
              : loginMethod === "PASSKEY"
                ? loginWithPasskey
                : undefined
          }
          type={loginMethod === "PASSWORD" ? "submit" : "button"}
        >
          {isPending
            ? "正在跳转..."
            : loginMethod === "PASSWORD"
              ? "密码登录"
              : loginMethod === "OTP"
                ? "验证并登录"
                : "使用 Passkey 登录"}
        </button>
      </form>
    </section>
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
