# API Walkthrough

这份文档从“接口 + 调用链”的角度理解当前系统，重点不是只列 endpoint，而是说明每条主链路里：

- 前端发了什么请求
- 后端 handler 是谁
- 核心逻辑在哪里
- 最终读写了哪些 db/cache

当前项目的典型链路可概括为：

`用户动作 -> 前端请求 -> route -> handler -> service / 核心逻辑 -> db/cache -> 返回结果`

## 1. Password 登录链路

### 用户动作

用户在登录页：

1. 选择 `subject_type`
2. 输入 `identifier`
3. 输入 `password`
4. 点击 Password 登录

### 前端请求

前端组件 [`login-form.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/components/login-form.tsx) 中的 `onSubmit` 会调用：

```text
POST /api/auth/password/login
```

请求体包含：

- `subject_type`
- `identifier`
- `password`

成功后前端会：

1. 保存 `access_token + refresh_token + subject + session`
2. 按 `subject_type` 跳转到对应 portal

### route

- `POST /api/auth/password/login`

定义在 [`routes.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/routes.rs)

### handler

- `password_login`

定义在 [`auth.rs`](/Users/martin/Downloads/workspace/Challenge-1/backend/src/auth.rs)

### service / 核心逻辑

`password_login` 当前主链路是：

1. 解析请求体
2. 解析 `subject_type`
3. 标准化 `identifier`
4. 调用 `find_subject_by_password`
5. 调用 `verify_password`
6. 读取 `User-Agent`
7. 调用 `create_session_and_tokens`
8. 写 `LOGIN_SUCCESS` 审计日志
9. 返回标准登录响应

### db/cache 操作

读取 SQLite：

- `subjects`
- `subject_identifiers`
- `password_credentials`

写入 SQLite：

- `devices`
- `sessions`
- `audit_logs`

不使用 Moka。

### 返回结果

返回统一 `AuthResponse`：

- `access_token`
- `refresh_token`
- `subject`
- `session`

## 2. OTP 登录链路

OTP 分成两段：

- request
- verify

### 2.1 OTP request

#### 用户动作

用户在登录页切换到 OTP，输入 `subject_type + identifier`，点击“获取验证码”。

#### 前端请求

前端 `requestOtpCode` 会调用：

```text
POST /api/auth/otp/request
```

#### route

- `POST /api/auth/otp/request`

#### handler

- `otp_request`

#### service / 核心逻辑

`otp_request` 当前主链路是：

1. 解析请求体
2. 解析 `subject_type`
3. 标准化 `identifier`
4. 调用 `enforce_rate_limit`
5. 调用 `find_subject_by_otp_identity`
6. 生成 `challenge_id + code + expires_at + attempts_left`
7. 写入 `otp_cache`
8. 写 `OTP_REQUESTED` 审计日志
9. 返回 challenge 信息

#### db/cache 操作

读取 SQLite：

- `subjects`
- `otp_identities`

写入 Moka：

- `otp_cache`
- `rate_limit_cache`

写入 SQLite：

- `audit_logs`

#### 返回结果

返回：

- `challenge_id`
- `channel_type`
- `masked_destination`
- `expires_at`
- `max_attempts`
- `dev_code`，开发模式下返回

### 2.2 OTP verify

#### 用户动作

用户输入验证码并提交。

#### 前端请求

前端 `verifyOtpCode` 会调用：

```text
POST /api/auth/otp/verify
```

请求体包含：

- `challenge_id`
- `code`

#### route

- `POST /api/auth/otp/verify`

#### handler

- `otp_verify`

#### service / 核心逻辑

`otp_verify` 当前主链路是：

1. 解析 `challenge_id` 和 `code`
2. 从 `otp_cache` 读取 challenge
3. 校验 challenge 是否存在
4. 校验是否过期
5. 校验 attempts 是否已耗尽
6. 校验 code 是否正确
7. 成功后删除 challenge
8. 调用 `create_session_and_tokens`
9. 写 `OTP_VERIFIED` 和 `LOGIN_SUCCESS`
10. 返回标准登录响应

#### db/cache 操作

读取 Moka：

- `otp_cache`

写回 Moka：

- 错误时更新 attempts
- 成功或失败上限时删除 challenge

写入 SQLite：

- `devices`
- `sessions`
- `audit_logs`

#### 返回结果

成功返回与 Password 登录一致：

- `access_token`
- `refresh_token`
- `subject`
- `session`

## 3. Refresh 链路

### 用户动作

当前前端没有显式 refresh 按钮，但后端 refresh 接口已经实现，代表 access token 续期路径。

### 前端请求

客户端若需要续期，应调用：

```text
POST /api/auth/refresh
```

请求体：

- `refresh_token`

### route

- `POST /api/auth/refresh`

### handler

- `refresh`

### service / 核心逻辑

`refresh` 当前主链路是：

1. 校验 `refresh_token`
2. 对 refresh token 做 hash
3. 调用 `find_active_session_by_refresh_hash`
4. 确认 session 状态仍为 `ACTIVE`
5. 生成新的 refresh token
6. 轮换 `refresh_token_hash`
7. 更新 `sessions.last_seen_at`
8. 更新 `devices.last_seen_at`
9. 重新签发 access token
10. 返回新的 token 对

### db/cache 操作

读取 SQLite：

- `sessions`
- `subjects`

写入 SQLite：

- `sessions`
- `devices`

不使用 Moka。

### 返回结果

返回新的：

- `access_token`
- `refresh_token`
- `subject`
- `session`

### 当前实现关键点

- refresh 不是纯 JWT 自续期
- refresh 强依赖服务端 active session
- session 被撤销后 refresh 必须失败

## 4. Session 列表与撤销链路

### 4.1 Session 列表

#### 用户动作

用户进入 portal 页面，或者在会话操作后刷新页面状态。

#### 前端请求

[`portal-client.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/components/portal-client.tsx) 初始化时会并发请求：

- `GET /api/auth/me`
- `GET /api/auth/sessions`
- `GET /api/portal/...`

session 列表请求本身是：

```text
GET /api/auth/sessions
```

#### route

- `GET /api/auth/sessions`

#### handler

- `list_sessions`

#### service / 核心逻辑

`list_sessions` 当前主链路是：

1. 认证当前请求
2. 识别当前 subject 和当前 session
3. 调用 `load_subject_sessions`
4. 查询当前主体的全部 session
5. 标记哪条是 `is_current`
6. 返回会话数组

#### db/cache 操作

读取 SQLite：

- `sessions`
- `devices`

不使用 Moka。

#### 返回结果

返回每条 session 的：

- `session_id`
- `device_id`
- `device_label`
- `user_agent`
- `login_method`
- `status`
- `created_at`
- `last_seen_at`
- `expires_at`
- `is_current`

### 4.2 撤销单个 session

#### 用户动作

用户在 portal 页面点击某一条会话的“撤销该会话”。

#### 前端请求

```text
DELETE /api/auth/sessions/:id
```

#### route

- `DELETE /api/auth/sessions/{id}`

#### handler

- `revoke_session`

#### service / 核心逻辑

`revoke_session` 当前主链路是：

1. 认证当前请求
2. 校验目标 session 是否属于当前主体
3. 校验其状态是否仍为 `ACTIVE`
4. 更新为 `REVOKED`
5. 返回成功结果

#### db/cache 操作

读取 SQLite：

- `sessions`

写入 SQLite：

- `sessions.status = REVOKED`

#### 返回结果

返回：

- `success`
- `message`
- `session_id`

## 5. Portal 访问链路

### 用户动作

用户登录成功后进入：

- `/member`
- `/community-staff`
- `/platform-staff`

### 前端请求

portal 页面加载时，请求：

- `GET /api/auth/me`
- `GET /api/auth/sessions`
- `GET /api/portal/<portal>`

portal 请求由 [`portal-client.tsx`](/Users/martin/Downloads/workspace/Challenge-1/frontend/components/portal-client.tsx) 触发。

### route

- `GET /api/portal/member/home`
- `GET /api/portal/community/home`
- `GET /api/portal/platform/home`

### handler

- `member_home`
- `community_home`
- `platform_home`

这三个 handler 最终会进入内部公共函数 `portal_home`。

### service / 核心逻辑

`portal_home` 当前主链路是：

1. 调用 `authenticate_bearer`
2. 确认当前 access token 对应有效 subject / session
3. 调用 `require_subject_type`
4. 如果主体类型不匹配：
   - 写 `PORTAL_ACCESS_DENIED`
   - 返回 `403 PORTAL_FORBIDDEN`
5. 如果匹配：
   - 组装 portal payload
   - 写 `PORTAL_ACCESS_GRANTED`
   - 返回 portal 数据

### db/cache 操作

读取 SQLite：

- `sessions`
- `subjects`

写入 SQLite：

- `audit_logs`

portal 示例数据本身当前是后端代码中组装的 payload，不是独立业务表查询结果。

### 返回结果

成功返回：

- `portal_key`
- `portal_title`
- `allowed_subject_type`
- `summary`
- `highlights`

失败返回：

- `403`
- `PORTAL_FORBIDDEN`

## 6. 一条总链路如何概括

这个项目当前最典型的调用链可以概括为：

1. 前端页面收集输入
2. 浏览器直接请求 Rust backend
3. route 把请求分发到 handler
4. handler 负责解析、鉴权入口和错误包装
5. handler 内部调用查询函数、校验函数和公共 session 创建逻辑
6. 长期状态落 SQLite，短期状态落 Moka
7. 返回统一 JSON
8. 前端决定本地存储、刷新页面或跳转 portal

## 7. 一句话总结

如果只看接口表，会觉得它是一些零散 auth API；但从调用链看，当前系统真正的重点是：不同认证方式最终统一收口到服务端 session，而 portal 边界再在此之上做最小授权控制。
