# Multi-Subject Auth System

## 项目简介

这是一个支持多主体认证与多设备会话管理的全栈系统样例。系统统一承载三类主体：

- `MEMBER`
- `COMMUNITY_STAFF`
- `PLATFORM_STAFF`

并在一套架构中实现：

- Password 登录
- OTP 登录
- Passkey 绑定与登录
- 服务端持久化 session
- 多设备会话管理
- 按主体类型区分的 portal 边界

项目目标不是做一个“最大而全”的生产级认证平台，而是在当前技术栈约束下，把主体建模、凭证建模、会话控制和认证/授权边界组织清楚。

## 技术栈

### Backend

- Rust
- Axum
- SQLx
- SQLite
- Moka

### Frontend

- Next.js App Router
- TypeScript
- Tailwind CSS

## 架构概览

当前系统的关键设计点是：

- 使用统一 `subjects` 模型承载多类主体，而不是三套独立账号体系
- 将 Password / OTP / Passkey 作为独立 credential 能力，而不是塞进主体主表
- 使用服务端持久化 session，而不是纯 JWT 会话
- 使用 Moka 保存 OTP challenge、Passkey challenge 和最小 rate-limit 窗口
- 将 portal 边界作为单独授权层，而不是混进认证逻辑

### 后端模块

- [`backend/src/main.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/main.rs)
  - 应用启动、SQLite 初始化、Moka 初始化、`AppState`
- [`backend/src/routes.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/routes.rs)
  - 路由装配
- [`backend/src/auth.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/auth.rs)
  - Password、OTP、token、session 管理主流程
- [`backend/src/passkey.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/passkey.rs)
  - Passkey register/login 流程
- [`backend/src/db/mod.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/db/mod.rs)
  - 数据库创建、migration 执行、CLI 初始化入口
- [`backend/src/db/seed.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/db/seed.rs)
  - 开发环境 seed 数据
- [`backend/src/access.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/access.rs)
  - portal 入口边界控制
- [`backend/src/portal.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/portal.rs)
  - member / community / platform 三个 portal API
- [`backend/src/audit.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/audit.rs)
  - 审计日志写入
- [`backend/src/rate_limit.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/rate_limit.rs)
  - Moka rate-limit

### 前端结构

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
- [`frontend/components/login-form.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/components/login-form.tsx)
  - Password / OTP / Passkey 登录交互
- [`frontend/components/portal-client.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/components/portal-client.tsx)
  - portal 页面公共逻辑、会话列表、Passkey 绑定

## 数据模型概览

当前 SQLite 核心表包括：

- `subjects`
  - 统一主体主表
- `subject_identifiers`
  - 登录标识，如 email / phone / member_no / staff_no
- `credentials`
  - 通用凭证主表，为后续扩展预留
- `password_credentials`
  - Password 凭证
- `otp_identities`
  - OTP 可达通道，不保存一次性验证码
- `passkey_credentials`
  - Passkey 凭证元数据
- `devices`
  - 登录设备
- `sessions`
  - 服务端持久化 session
- `audit_logs`
  - 认证与访问相关审计事件

## 认证方式说明

### Password

- 长期状态落在 `password_credentials`
- 后端逻辑集中在 `auth.rs`
- 登录成功后统一创建 device + session

### OTP

- 可达通道落在 `otp_identities`
- 一次性验证码不落库
- challenge / expiration / attempts 落在 Moka `otp_cache`
- 验证成功后同样进入统一 session 创建逻辑

### Passkey

- 长期状态落在 `passkey_credentials`
- challenge 落在 Moka `passkey_cache`
- 前端通过 WebAuthn API 发起浏览器交互
- 登录成功后同样复用统一 session 创建逻辑

## Session 模型说明

当前项目的 session 不是纯 JWT 伪实现，而是服务端真实记录。

原因是系统要支持：

- 多设备登录
- session 列表
- 当前设备登出
- 全部设备登出
- 撤销单个 session
- refresh 检查 session 是否仍为 active

当前实现中：

- access token 是 JWT
- refresh token 只返回一次明文
- 数据库里只保存 `refresh_token_hash`
- 真正的生命周期状态在 `sessions`

## Database Setup

数据库文件不会提交到仓库。

当前实现中，backend 启动时会自动完成：

1. 创建 SQLite 数据库文件
2. 执行 SQLx migrations
3. 在开发环境插入 seed 数据

### migration 机制

- migration 文件位于 [`backend/migrations`](/Users/martin/Downloads/workspace/Challenge-1/backend/migrations)
- backend 使用 `sqlx::migrate!()` 在启动时执行 migration
- schema 使用纯 SQL 文件维护

### 自动初始化逻辑

当前 backend 启动时会：

1. 检查数据库文件是否存在
2. 如果不存在则创建数据库
3. 连接数据库
4. 执行 migration
5. 如果 `APP_ENV=development`，自动执行 seed

### 手动初始化方式

初始化数据库并执行 migration：

```bash
cd backend
cargo run -- init-db
```

手动执行 seed：

```bash
cd backend
cargo run -- seed
```

常规启动：

```bash
cd backend
cargo run
```

## 如何启动

### 环境要求

- Rust / Cargo
- Node.js 22+
- npm 11+

### 启动 backend

```bash
cd backend
cargo run
```

默认地址：

```text
http://127.0.0.1:3001
```

健康检查：

```bash
curl http://127.0.0.1:3001/health
```

### 启动 frontend

```bash
cd frontend
npm install
npm run dev
```

推荐访问：

```text
http://localhost:3000
```

登录页：

```text
http://localhost:3000/login
```

如果 `127.0.0.1:3000` 返回 404，而 `localhost:3000` 正常，通常意味着本机还有其他服务占用了 IPv4 `3000` 端口。这种情况下优先使用 `localhost:3000`，或者改用其他端口启动前端。

### 构建验证

backend:

```bash
cd backend
cargo build
```

frontend:

```bash
cd frontend
npm run build
```

## 测试账号

后端启动时会自动初始化最小 schema 和 seed 数据。

### Member

- `subject_type`: `MEMBER`
- `identifier`: `member@example.com`
- `identifier`: `13800000001`
- `identifier`: `member001`
- `password`: `Password123!`

### Community Staff

- `subject_type`: `COMMUNITY_STAFF`
- `identifier`: `community.staff@example.com`
- `identifier`: `cstaff001`
- `password`: `Password123!`

### Platform Staff

- `subject_type`: `PLATFORM_STAFF`
- `identifier`: `platform.staff@example.com`
- `identifier`: `pstaff001`
- `password`: `Password123!`

## 演示脚本

下面这组步骤适合快速说明系统主路径。

### 1. Password 登录

1. 打开 `/login`
2. 选择 `MEMBER`
3. 输入 `member@example.com / Password123!`
4. 登录后进入 `/member`

重点观察：

- 统一登录入口
- 登录成功创建 device + session
- 按 `subject_type` 跳转 portal

### 2. OTP 登录

1. 切换到 `OTP`
2. 输入 `MEMBER / member@example.com`
3. 请求验证码
4. 使用开发模式返回的 `dev_code`
5. 完成登录

重点观察：

- `otp_identities` 只表示可达通道
- code 不落库，challenge 在 Moka
- 成功后仍进入统一 session 逻辑

### 3. 多设备 session

1. 在普通窗口登录 Member
2. 在无痕窗口再次登录同一 Member
3. 回到 portal 查看 session 列表

重点观察：

- 同一主体下有多条 active session
- 当前设备与其他设备可以区分

### 4. 撤销单个 session

1. 在 session 列表里找到非当前设备
2. 点击撤销

重点观察：

- session 状态变化
- 被撤销 session 后续 refresh 会失败

### 5. 全部登出

1. 点击“全部登出”

重点观察：

- 当前主体所有 active session 被统一失效

### 6. Portal 边界

1. 用 Member 登录
2. 访问 `GET /api/portal/community/home`

重点观察：

- 返回 `403 PORTAL_FORBIDDEN`
- 说明认证成功后，仍有单独授权边界

### 7. Passkey

1. 登录进入任意 portal
2. 绑定 Passkey
3. 回到登录页使用 Passkey 登录

重点观察：

- register/login challenge 在 Moka
- Passkey 最终也复用统一 session 创建逻辑

## 已实现 / 未实现

### 已实现

- 三类主体统一建模
- Password 登录
- OTP 登录
- Passkey 绑定与登录闭环
- Access Token + Refresh Token
- 多设备 session 管理
- Session 列表
- 当前设备登出
- 全部设备登出
- 撤销单个 session
- portal 边界
- 基础 audit log
- Moka challenge cache
- Moka 最小 rate-limit

### 未完全实现

- 生产级 Passkey attestation / assertion 验签
- 完整 MFA 编排
- 复杂 RBAC / ABAC
- 审计查询页面
- Passkey 管理页面
- 第三方 OTP 投递服务

## 后续扩展方向

- 引入 login flow / auth challenge 层，扩展到 MFA
- 增加 role / permission / policy 层
- 增强 Passkey 管理能力
- 增强审计查询与筛选
- 接入真实邮件 / 短信 OTP 通道
- 视部署需求将短期状态从进程内缓存扩展到分布式方案

## 仓库结构说明

```text
backend/
  migrations/
  src/
    main.rs
    db/
      mod.rs
      seed.rs
    routes.rs
    auth.rs
    passkey.rs
    access.rs
    portal.rs
    audit.rs
    rate_limit.rs

frontend/
  app/
  components/
  lib/

docs/
  architecture-guide.md
  api-walkthrough.md
  operation-guide.md
```

## 当前实现与原始设计目标的关系

原始设计目标在 [`PLAN.md`](/Users/martin/Downloads/workspace/Challenge-1/PLAN.md) 中定义得更宽，当前代码已经完成主路径，但仍有部分内容保持在“结构预留”而不是“完整产品化实现”的层次。

当前已经达到的核心目标是：

- 多主体
- 多认证方式
- 多设备 session
- portal 边界
- SQLite + Moka 的状态分层

仍需要诚实说明的部分是：

- Passkey 不是生产级安全实现
- MFA 尚未真正编排落地
- 权限系统只做到 portal boundary
