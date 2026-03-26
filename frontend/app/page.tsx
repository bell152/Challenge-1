const backendUrl =
  process.env.NEXT_PUBLIC_BACKEND_URL ?? "http://127.0.0.1:3001";

import Link from "next/link";

const checkpoints = [
  "Password / OTP / Passkey",
  "Persistent server-side sessions",
  "Subject-based portal boundary",
  "SQLite + Moka only",
  "Architecture and operation guide",
];

export default function HomePage() {
  return (
    <main className="mx-auto flex min-h-screen max-w-5xl flex-col justify-center px-6 py-16">
      <section className="rounded-[32px] border border-slate-200/70 bg-white/85 p-8 shadow-[0_20px_80px_rgba(15,23,42,0.08)] backdrop-blur">
        <p className="text-sm font-semibold uppercase tracking-[0.3em] text-sky-700">
          System Overview
        </p>
        <h1 className="mt-4 max-w-3xl text-4xl font-semibold tracking-tight text-slate-950 md:text-6xl">
          Multi-Subject Auth Architecture
        </h1>
        <p className="mt-6 max-w-2xl text-lg leading-8 text-slate-600">
          这是一个用于说明多主体认证系统设计的样例实现。项目重点不是堆砌功能，
          而是在本地可运行的前提下，把主体建模、认证方式、会话管理和 portal
          边界组织清楚。
        </p>

        <div className="mt-10 grid gap-4 md:grid-cols-2">
          {checkpoints.map((item) => (
            <div
              key={item}
              className="rounded-2xl border border-slate-200 bg-slate-50/80 px-4 py-4 text-sm text-slate-700"
            >
              {item}
            </div>
          ))}
        </div>

        <div className="mt-10 rounded-2xl bg-slate-950 px-5 py-4 text-sm text-slate-100">
          <p className="font-medium text-white">Backend health endpoint</p>
          <p className="mt-2 break-all text-slate-300">{backendUrl}/health</p>
        </div>

        <div className="mt-6 flex flex-wrap gap-3">
          <Link
            className="rounded-2xl bg-sky-600 px-5 py-3 text-sm font-medium text-white transition hover:bg-sky-500"
            href="/login"
          >
            打开登录页
          </Link>
          <Link
            className="rounded-2xl border border-slate-200 px-5 py-3 text-sm font-medium text-slate-700 transition hover:border-slate-300 hover:bg-slate-50"
            href={backendUrl}
          >
            Backend Base URL
          </Link>
        </div>
      </section>
    </main>
  );
}
