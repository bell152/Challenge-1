# Operation Guide

This document describes how to run the system and walk through the main flows using the current implementation.

## 1. Start The System

### Backend

```bash
cd backend
cargo run
```

Expected backend URL:

```text
http://127.0.0.1:3001
```

Health check:

```bash
curl http://127.0.0.1:3001/health
```

### Frontend

```bash
cd frontend
npm install
npm run dev
```

Recommended URL:

```text
http://localhost:3000
```

Login page:

```text
http://localhost:3000/login
```

If `127.0.0.1:3000` returns 404 while `localhost:3000` works, another local process is probably bound to IPv4 port `3000`.

## 2. Seed Accounts

### Member

- `MEMBER / member@example.com / Password123!`

### Community Staff

- `COMMUNITY_STAFF / community.staff@example.com / Password123!`

### Platform Staff

- `PLATFORM_STAFF / platform.staff@example.com / Password123!`

## 3. Stable Walkthrough Path

If only one path is needed, use this order:

1. Password login
2. session list
3. second browser login
4. revoke one session
5. logout all
6. portal boundary check

This path exercises the core architectural decisions with the lowest runtime risk.

## 4. Walkthrough Details

### 4.1 Password Login

Steps:

1. Open `/login`
2. Select `MEMBER`
3. Use `member@example.com`
4. Use `Password123!`
5. Submit Password login

Expected result:

- redirected to `/member`
- current subject visible
- session list visible
- portal payload visible

### 4.2 Multiple Devices

Steps:

1. Keep the first browser session open
2. Open an incognito window
3. Log in with the same Member account
4. Return to a portal page and refresh

Expected result:

- more than one active session
- one marked as current
- another marked as non-current

### 4.3 Revoke One Session

Steps:

1. In the session list, find a non-current session
2. Revoke it

Expected result:

- the session state changes
- the revoked session cannot refresh successfully afterward

### 4.4 Logout All

Steps:

1. Use the `logout all` action on a portal page

Expected result:

- all active sessions for the subject are marked logged out

### 4.5 Portal Boundary

Steps:

1. Log in as `MEMBER`
2. Use the access token against the Community portal API

Example:

```bash
curl -i http://127.0.0.1:3001/api/portal/community/home \
  -H "Authorization: Bearer <access_token>"
```

Expected result:

- `403 PORTAL_FORBIDDEN`

## 5. OTP Flow

Steps:

1. Return to `/login`
2. Switch to `OTP`
3. Select a valid subject type and identifier
4. Request a code
5. Use the returned `dev_code` in development mode
6. Verify the OTP

Expected result:

- successful login
- new session created with `login_method = OTP`

## 6. Passkey Flow

### Current implementation note

Passkey is implemented as a working sample flow, but not as a production-grade WebAuthn stack.

### Register

Steps:

1. Log in with any subject
2. Open the portal page
3. Click `Bind Passkey`

Expected result:

- browser invokes WebAuthn
- a Passkey credential record is stored

### Login

Steps:

1. Return to `/login`
2. Switch to `Passkey`
3. Use the same subject type and identifier
4. Complete the browser Passkey flow

Expected result:

- successful login
- new `PASSKEY` session created

## 7. Rate-Limit Check

The current repository includes a minimal Moka-based rate-limit implementation.

### OTP request

For the same `subject_type + identifier`:

- first 3 requests succeed within the window
- the 4th request returns `429 OTP_REQUEST_RATE_LIMITED`

### Passkey options

For the same subject or subject identifier:

- repeated requests to register/login options eventually return `429`

## 8. Known Runtime Caveats

- Passkey depends on browser and secure-context support
- OTP code display relies on development mode
- rate-limit is in-memory and resets when the backend restarts
- frontend `localhost` may work while `127.0.0.1` does not if another service occupies IPv4 port `3000`

## 9. Recommended Reading Order

1. [README.md](/Users/martin/Downloads/workspace/Challenge-1/README.md)
2. [architecture-guide.md](/Users/martin/Downloads/workspace/Challenge-1/docs/architecture-guide.md)
3. [api-walkthrough.md](/Users/martin/Downloads/workspace/Challenge-1/docs/api-walkthrough.md)
4. this document
