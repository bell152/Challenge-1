# Architecture Guide

这份文档用于快速理解当前项目的整体架构，并明确区分：

- 原始设计目标是什么
- 当前代码已经实现了什么
- 哪些能力还只是后续扩展方向

## 1. 项目一句话简介

这是一个支持多主体登录与多设备会话管理的认证系统样例，统一承载 `MEMBER`、`COMMUNITY_STAFF`、`PLATFORM_STAFF` 三类主体，并在一套架构中实现 Password、OTP、Passkey 三种认证入口、服务端持久化 session，以及按主体类型区分的 portal 边界。

## 2. 为什么使用统一 Subject 模型

当前系统没有为三类主体建立三套独立账号体系，而是统一使用 `subjects` 主表。

原因是三类主体的核心差异并不在认证基础设施本身，而主要在：

- 主体类型 `subject_type`
- 可用标识符
- 可访问的 portal

如果拆成三套系统，会重复建设：

- Password 登录
- OTP 登录
- Passkey 逻辑
- session 生命周期
- audit log
- 前端登录入口与跳转逻辑

当前实现把共性收敛到统一 subject 域，把差异落在：

- `subject_type`
- `subject_identifiers`
- portal access boundary

## 3. 为什么 credentials 独立建模

当前系统没有把密码、OTP、Passkey 直接塞进 `subjects` 主表，而是拆成独立 credential 模型。

当前表结构里与认证直接相关的部分是：

- `credentials`
- `password_credentials`
- `otp_identities`
- `passkey_credentials`

这样设计的原因是：

- 主体信息与认证凭据解耦
- 不同凭据类型可以独立扩展
- OTP 可达通道和一次性验证码不是同一种数据
- Passkey 的结构天然不同于 Password 和 OTP

在当前实现里：

- Password 的长期状态放在 `password_credentials`
- OTP 的长期状态放在 `otp_identities`
- OTP 的短期验证码状态放在 Moka `otp_cache`
- Passkey 的长期状态放在 `passkey_credentials`
- Passkey 的短期 challenge 放在 Moka `passkey_cache`

## 4. 为什么 sessions 要服务端持久化

当前系统明确支持：

- 多设备登录
- session 列表
- 当前设备登出
- 全部设备登出
- 撤销单个 session
- refresh 必须依赖 active session

这些能力如果只依赖纯 JWT，会比较别扭，因为 JWT 天然偏无状态，难以优雅表达：

- 某个设备已经被撤销
- 只踢掉一个 session
- refresh token 对应的 session 是否仍为 `ACTIVE`

所以当前实现中：

- access token 是短期 JWT
- refresh token 是续期凭据
- 真实会话状态落在 `sessions`
- 登录成功时还会同步创建 `devices`

一句话概括：

JWT 解决“凭证携带”，session 解决“会话控制”。

## 5. Password / OTP / Passkey 的架构位置

三种认证方式都属于 Authentication 域，但在代码中的层次不同。

### Password

- 持久化层：
  - `password_credentials`
- 后端主流程：
  - [`auth.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/auth.rs)
- 前端交互：
  - [`login-form.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/components/login-form.tsx)

### OTP

- 持久化层：
  - `otp_identities`
- 短期缓存层：
  - `otp_cache`
- 后端主流程：
  - [`auth.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/auth.rs)
- 前端交互：
  - [`login-form.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/components/login-form.tsx)

### Passkey

- 持久化层：
  - `passkey_credentials`
- 短期缓存层：
  - `passkey_cache`
- 后端主流程：
  - [`passkey.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/passkey.rs)
- 前端浏览器适配层：
  - [`frontend/lib/passkey.ts`](/Users/martin/Downloads/workspace/Challenge-1/frontend/lib/passkey.ts)
- 前端交互：
  - [`login-form.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/components/login-form.tsx)
  - [`portal-client.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/components/portal-client.tsx)

## 6. 认证与授权的边界

### 认证 Authentication

当前项目中，下面这些属于认证：

- Password 校验
- OTP request / verify
- Passkey register / login verify
- access token 解码
- refresh token 映射到 active session 的校验
- `authenticate_request` / `authenticate_bearer`

认证回答的是：

- 你是谁
- 你当前的 credential / token / session 是否有效

### 授权 Authorization

当前项目中的授权做得刻意比较轻，只做 portal 入口边界：

- Member 只能进 Member portal
- Community Staff 只能进 Community portal
- Platform Staff 只能进 Platform portal

这层边界被单独放在：

- [`access.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/access.rs)
- [`portal.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/portal.rs)

当前系统中：

- Authentication 负责确认主体身份
- Authorization 负责限制 portal 入口

`subject_type` 是主体边界，不是完整权限系统。

## 7. 当前后端模块划分

### [`backend/src/main.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/main.rs)

- 应用启动入口
- 初始化 SQLite
- 初始化 Moka 缓存
- 构建 `AppState`
- 暴露 `/health`

### [`backend/src/routes.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/routes.rs)

- 路由装配
- 将 HTTP route 显式绑定到 handler

### [`backend/src/auth.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/auth.rs)

- Password 登录
- OTP request / verify
- refresh
- logout
- logout-all
- `me`
- sessions list
- revoke session
- 公共 session / token 创建逻辑

### [`backend/src/passkey.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/passkey.rs)

- Passkey register options / verify
- Passkey login options / verify

### [`backend/src/db/mod.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/db/mod.rs)

- 数据库创建
- migration 执行
- 启动时自动初始化
- CLI 初始化命令解析

### [`backend/src/db/seed.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/db/seed.rs)

- 开发环境 seed 数据
- 幂等插入测试主体、标识符、OTP 通道和密码凭证

### [`backend/src/access.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/access.rs)

- portal 入口的最小授权边界

### [`backend/src/portal.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/portal.rs)

- 三类 portal API
- portal 访问审计
- portal 页面示例数据

### [`backend/src/audit.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/audit.rs)

- 审计日志写入

### [`backend/src/rate_limit.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/rate_limit.rs)

- 基于 Moka 的最小可运行 rate-limit

## 8. 当前前端页面结构

### 页面入口

- [`frontend/app/page.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/app/page.tsx)
  - 系统首页
- [`frontend/app/login/page.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/app/login/page.tsx)
  - 登录页
- [`frontend/app/member/page.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/app/member/page.tsx)
  - Member portal
- [`frontend/app/community-staff/page.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/app/community-staff/page.tsx)
  - Community Staff portal
- [`frontend/app/platform-staff/page.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/app/platform-staff/page.tsx)
  - Platform Staff portal

### 组件与辅助模块

- [`frontend/components/login-form.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/components/login-form.tsx)
  - Password / OTP / Passkey 登录交互
- [`frontend/components/portal-client.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/components/portal-client.tsx)
  - portal 公共逻辑、会话列表、登出、Passkey 绑定
- [`frontend/lib/auth.ts`](/Users/martin/Downloads/workspace/Challenge-1/frontend/lib/auth.ts)
  - 本地会话存储、类型定义、portal 路由映射
- [`frontend/lib/passkey.ts`](/Users/martin/Downloads/workspace/Challenge-1/frontend/lib/passkey.ts)
  - WebAuthn 编解码和浏览器适配

## 9. 请求链路：route -> handler -> service -> db/cache

当前项目虽然没有单独命名为 `service/` 目录，但已经具备清晰的处理链路。

### 示例 1：Password 登录

- route:
  - `POST /api/auth/password/login`
- handler:
  - `password_login`
- service / 核心逻辑：
  - `find_subject_by_password`
  - `verify_password`
  - `create_session_and_tokens`
- db/cache：
  - 读 `subjects` / `subject_identifiers` / `password_credentials`
  - 写 `devices` / `sessions` / `audit_logs`

### 示例 2：OTP request

- route:
  - `POST /api/auth/otp/request`
- handler:
  - `otp_request`
- service / 核心逻辑：
  - `enforce_rate_limit`
  - `find_subject_by_otp_identity`
  - challenge 生成逻辑
- db/cache：
  - 读 `otp_identities`
  - 写 `otp_cache`
  - 写 `rate_limit_cache`
  - 写 `audit_logs`

### 示例 3：Portal 访问

- route:
  - `GET /api/portal/member/home`
- handler:
  - `member_home`
- service / 核心逻辑：
  - `authenticate_bearer`
  - `require_subject_type`
  - `portal_payload`
- db/cache：
  - 读 `sessions` / `subjects`
  - 写 `audit_logs`

## 10. 当前实现 vs 原始设计目标

### 原始设计目标

根据 [`PLAN.md`](/Users/martin/Downloads/workspace/Challenge-1/PLAN.md)，项目目标包括：

- 多主体统一建模
- Password / OTP / Passkey
- Access Token + Refresh Token
- 多设备 session 管理
- Portal 边界
- Moka 承担 challenge / rate-limit 等短期状态
- 为 MFA 预留扩展能力

### 当前已经实现

- 统一 `subjects` 模型
- Password 登录
- OTP 登录
- Passkey 绑定与登录闭环
- server-side sessions
- refresh / logout / logout-all / session list / revoke session
- portal boundary
- audit baseline
- Moka challenge cache + minimal rate-limit

### 当前仍未完全达到原始设计目标的部分

- Passkey 仍不是生产级 WebAuthn 安全实现
- MFA 只是结构上可扩展，尚未真正实现编排
- 权限系统仍停留在 portal boundary，而不是 RBAC / ABAC
- audit 目前只有基础写入，没有查询能力

## 11. 后续如何扩展 MFA

当前代码结构已经适合往 MFA 扩展，原因是：

- subject 模型已经统一
- credential 已经拆分
- session 创建逻辑已经集中
- portal 授权与认证已分离

一个合理的扩展方式是：

1. first factor 成功后，不立即创建最终 active session
2. 先创建一个临时 login flow / auth challenge 状态
3. 根据策略要求 second factor
4. second factor 成功后，再调用统一 session 创建逻辑

这样扩展的好处是：

- 不需要推翻当前 subject / credential 设计
- 不需要改 portal 边界模型
- 可以逐步把 Password + OTP 或 Password + Passkey 组合成 MFA

## 12. 一句话总结

当前系统最核心的价值不在于“功能数量”，而在于把多主体、凭证解耦、服务端 session、短期缓存状态和 portal 边界这些关键设计点组织成了一套清楚、可运行、可扩展的实现。
