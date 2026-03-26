# PLAN.md

# Multi-Subject Auth System

一个面向系统设计说明的全栈项目，目标是实现一个支持多主体、多凭证、多设备会话管理的认证系统。

---

## 1. 项目目标

实现一个可本地运行、可验证的认证与会话管理系统，支持三类主体：

- Member
- Community Staff
- Platform Staff

核心要求：

- 三类主体独立登录
- 支持多设备会话
- 支持多种认证方式：
  - Password
  - OTP
  - Passkey
- 预留 MFA 扩展能力
- 提供完整前后端
- 强调建模能力、边界感、实现完整度、可验证性

---

## 2. 技术栈

### Backend
- Rust
- Axum
- Tokio
- SQLx
- SQLite
- Moka
- Serde
- tower-http
- tracing

### Frontend
- Next.js
- TypeScript
- Tailwind CSS

### 数据与缓存
- SQLite 作为持久化数据库
- Moka 作为进程内缓存
- 不引入 Redis / Postgres 等额外基础设施

---

## 3. 设计原则

### 3.1 统一 Subject 模型
系统不为三类主体做三套独立账号体系，而是统一抽象为 `subjects`。

### 3.2 认证与授权分离
- Authentication：解决“你是谁”
- Authorization：解决“你能做什么”

### 3.3 凭证独立建模
Password、OTP、Passkey 不直接塞进主体主表，而是通过 credential 模型扩展。

### 3.4 Session 是一等公民
为了支持多设备登录、单设备登出、全部登出、会话列表等能力，必须有服务端持久化 session。

### 3.5 MFA 是认证流程阶段
MFA 不是单独一种凭证，而是认证流程中的可扩展阶段。

### 3.6 设计导向
优先保证：
- 可运行
- 结构清楚
- 可以验证
- README 清晰解释设计取舍

---

## 4. 非目标

以下内容不是本项目当前阶段的重点：

- 第三方 OAuth / SSO
- 复杂 RBAC / ABAC 系统
- 生产级短信 / 邮件服务接入
- 分布式会话共享
- 风控引擎
- 多租户权限系统
- 高并发优化
- 完整安全合规建设

---

## 5. 核心领域建模

---

### 5.1 Subject

统一主体表：

- subject_id
- subject_type
- status
- display_name
- timestamps

`subject_type`:
- MEMBER
- COMMUNITY_STAFF
- PLATFORM_STAFF

---

### 5.2 Subject Identifiers

用于登录识别：

- EMAIL
- PHONE
- MEMBER_NO
- STAFF_NO

说明：
- Member 可使用 phone/email/member_no
- Staff 可使用 email/staff_no

---

### 5.3 Profile 扩展表

主体资料放扩展表，不污染认证主模型：

- members
- community_staff_profiles
- platform_staff_profiles

---

### 5.4 Credentials

统一凭证主表，按类型扩展：

- PASSWORD
- OTP
- PASSKEY

子模型包括：

- password_credentials
- otp_identities
- passkey_credentials

---

### 5.5 Devices

设备作为独立概念建模，用于支撑：

- 多设备登录
- 当前设备识别
- 设备信任
- 会话归属

---

### 5.6 Sessions

服务端持久化 session，支撑：

- refresh token
- 多设备列表
- 单设备登出
- 全部登出
- 审计与追踪

---

### 5.7 Audit Logs

记录关键安全行为，例如：

- LOGIN_SUCCESS
- LOGIN_FAILED
- OTP_REQUESTED
- OTP_VERIFIED
- SESSION_REVOKED
- LOGOUT_ALL

---

## 6. 数据模型范围

计划包含以下主要表：

- `subjects`
- `subject_identifiers`
- `members`
- `community_staff_profiles`
- `platform_staff_profiles`
- `credentials`
- `password_credentials`
- `otp_identities`
- `passkey_credentials`（可先占位）
- `devices`
- `sessions`
- `audit_logs`

---

## 7. 认证方式范围

### 7.1 Password
必须完整实现。

### 7.2 OTP
必须完整实现，开发环境下返回 `dev_code` 便于本地验证。

### 7.3 Passkey
加分项，尽量实现注册与登录；若时间不够，至少保留完整建模与接口骨架。

### 7.4 MFA
本阶段不要求完整产品化实现，但要在代码结构中预留扩展点。

---

## 8. Token 与 Session 策略

### Access Token
- JWT
- 短期有效，例如 15 分钟
- 包含：
  - sub
  - session_id
  - subject_type
  - exp

### Refresh Token
- 随机字符串
- 长期有效，例如 7 天
- 服务端 `sessions` 表中只存 hash

### Session 规则
- 登录成功后创建 session
- refresh 依赖 active session
- logout 撤销当前 session
- logout-all 撤销当前主体所有 active session
- 删除某个 session 后，该设备 refresh 必须失败

---

## 9. API 范围

### Auth
- `POST /api/auth/password/login`
- `POST /api/auth/otp/request`
- `POST /api/auth/otp/verify`
- `POST /api/auth/refresh`
- `POST /api/auth/logout`
- `POST /api/auth/logout-all`
- `GET /api/auth/me`
- `GET /api/auth/sessions`
- `DELETE /api/auth/sessions/:id`

### Passkey
- `POST /api/auth/passkey/register/options`
- `POST /api/auth/passkey/register/verify`
- `POST /api/auth/passkey/login/options`
- `POST /api/auth/passkey/login/verify`

### Portal
- `GET /api/portal/member/home`
- `GET /api/portal/community/home`
- `GET /api/portal/platform/home`

### Infra
- `GET /health`

---

## 10. 前端范围

### 登录页
路径建议：`/login`

功能：
- 选择主体类型
- 选择登录方式：
  - Password
  - OTP
  - Passkey
- 提交认证请求
- 登录成功后按主体跳转

### Portal 页面
- `/member`
- `/community-staff`
- `/platform-staff`

页面展示：
- 当前主体信息
- 当前主体类型
- 当前会话列表
- 当前登录方式
- 单设备登出
- 全部登出

### 安全中心
可与 portal 合并，但至少要有：
- session 列表
- 撤销 session
- Passkey 绑定入口（如实现）

---

## 11. 阶段拆分

---

### 阶段 0：项目骨架

#### 目标
创建前后端工程骨架，确保项目可启动。

#### 包含
- backend 工程初始化
- frontend 工程初始化
- `/health` 接口
- SQLite 连接初始化
- Moka 初始化
- README 初版

#### 完成定义
- backend 能运行
- frontend 能运行
- `/health` 返回成功
- 目录结构清晰

---

### 阶段 1：数据库模型与 seed

#### 目标
完成核心 schema 与开发环境种子数据。

#### 包含
- 建表
- migration 或自动初始化
- seed 三类测试账号
- argon2 密码 hash

#### 完成定义
- SQLite 中存在完整表结构
- 自动或一键生成测试账号
- README 补充测试账号说明

---

### 阶段 2：Password 登录闭环

#### 目标
打通第一条完整认证路径。

#### 包含
- password login
- me 接口
- device 创建
- session 创建
- token 签发
- 前端 password 登录

#### 完成定义
- 三类主体都能用 password 登录
- 登录后能进入对应 portal
- session 表有记录

---

### 阶段 3：Session 管理

#### 目标
实现多设备能力。

#### 包含
- refresh
- logout
- logout-all
- session list
- revoke session

#### 完成定义
- 同一账号支持多浏览器登录
- 可查看多个 session
- 可撤销某一个 session
- 被撤销设备 refresh 失败

---

### 阶段 4：OTP 登录

#### 目标
增加第二条完整认证路径。

#### 包含
- OTP request
- OTP verify
- Moka 缓存 challenge / code / attempts
- dev 模式返回 `dev_code`
- 前端 OTP 流程

#### 完成定义
- 至少一类主体可完成 OTP 登录
- 最终最好三类主体都支持
- OTP 支持过期与最大尝试次数
- OTP 登录后同样创建设备与 session

---

### 阶段 5：Portal 边界与审计

#### 目标
增强系统边界感。

#### 包含
- 三个 portal API
- subject_type 访问限制
- 基础审计日志
- 前端 portal 展示门户示例数据

#### 完成定义
- 错误主体不能访问错误 portal
- 审计日志可记录关键行为
- README 可解释认证与授权分离

---

### 阶段 6：Passkey

#### 目标
作为加分项实现或占位。

#### 包含
优先级 1：
- passkey register options
- passkey register verify

优先级 2：
- passkey login options
- passkey login verify

#### 完成定义
二选一：
1. 可完整验证 Passkey 注册与登录
2. 或结构完整、接口齐全、README 诚实说明未完成点

---

### 阶段 7：收尾与系统说明优化

#### 目标
将项目整理成可提交作品。

#### 包含
- 清理代码
- 修复构建错误
- 完整 README
- 增加操作验证脚本
- 增加架构说明
- 增加已实现 / 未实现 / 后续扩展

#### 完成定义
- clone 后按 README 可运行
- 适合用于系统说明与功能验证
- 设计取舍表达清晰

---

## 12. 已知风险

### 12.1 Passkey 集成复杂度
WebAuthn 本地调试、RP ID、浏览器兼容性会增加复杂度。

### 12.2 SQLite 能力边界
SQLite 足够支持本项目 MVP，但不适合作为生产级多实例并发会话共享方案。

### 12.3 Moka 为进程内缓存
适合样例运行和单机开发，但不适合多实例共享 OTP / challenge。

### 12.4 时间优先级冲突
若时间不足，优先级应为：

1. 项目骨架
2. 数据模型
3. Password 登录
4. Session 管理
5. OTP 登录
6. Portal 边界
7. Passkey
8. MFA 扩展

---

## 13. 开发优先级

### P0
必须完成：
- Subject 统一建模
- Password 登录
- Session 管理
- 多设备支持
- OTP 登录
- 前端可运行并可验证
- README

### P1
建议完成：
- 审计日志
- Portal 边界
- 简单 subject guard

### P2
加分项：
- Passkey
- MFA 流程预留增强
- 更丰富的安全中心

---

## 14. 验收标准

项目最终应满足：

- 可以本地启动 backend 和 frontend
- SQLite 可自动初始化或一键初始化
- 三类测试账号可用
- Password 登录可用
- OTP 登录可用
- Session 管理可用
- 单设备登出 / 全部登出可用
- 前端区分三类主体
- 后端结构清晰
- README 适合用于架构说明

---

## 15. 操作验证脚本

### 1
Member 使用 Password 登录

### 2
Community Staff 使用 OTP 登录

### 3
同一 Member 在两个浏览器登录，查看 session 列表并撤销其中一个

### 4
Platform Staff 登录并进入独立 portal

### 5
如果实现了 Passkey，验证 Passkey 绑定与登录

---

## 16. 测试账号规划

开发环境 seed：

### Member
- member_no: `member001`
- email: `member@example.com`
- phone: `13800000001`
- password: `Password123!`

### Community Staff
- staff_no: `cstaff001`
- email: `community.staff@example.com`
- password: `Password123!`

### Platform Staff
- staff_no: `pstaff001`
- email: `platform.staff@example.com`
- password: `Password123!`

说明：
- OTP 在 dev 模式下返回 `dev_code`
- 真实短信 / 邮件通道不在本阶段实现范围内

---

## 17. 对 Codex 的工作要求

Codex 在每个阶段都应：

1. 先给出简短计划
2. 再修改代码
3. 运行构建 / 测试 / 基本校验
4. 修复明显错误
5. 更新 README 或进度说明
6. 输出：
   - 本阶段完成内容
   - 未完成内容
   - 验证方式
   - 下一阶段建议

请避免：
- 一次性生成所有复杂功能但无法运行
- 把所有逻辑塞进 handler
- 过度抽象
- 引入不必要基础设施

---

## 18. 成功标准总结

这是一个“小而完整”的项目，而不是“大而空”的系统设计作业。

成功标准不是功能最多，而是：

- 建模清楚
- 代码可运行
- 会话能力完整
- 认证边界清楚
- 可用于系统说明与能力验证
- README 能讲清楚设计思路

---
