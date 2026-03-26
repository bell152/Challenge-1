"use client";

export type SubjectType = "MEMBER" | "COMMUNITY_STAFF" | "PLATFORM_STAFF";

export type SubjectInfo = {
  id: string;
  subject_type: SubjectType;
  display_name: string;
  status: string;
};

export type SessionInfo = {
  session_id: string;
  device_id: string;
  device_label: string;
  user_agent: string;
  login_method: string;
  status: string;
  created_at: string;
  last_seen_at: string;
  expires_at: string;
  is_current: boolean;
};

export type LoginResponse = {
  access_token: string;
  refresh_token: string;
  subject: SubjectInfo;
  session: SessionInfo;
};

export type OtpRequestResponse = {
  challenge_id: string;
  channel_type: string;
  masked_destination: string;
  expires_at: string;
  max_attempts: number;
  dev_code?: string;
};

export type MeResponse = {
  subject: SubjectInfo;
  session: SessionInfo;
};

export type SessionsResponse = {
  sessions: SessionInfo[];
};

export type ActionResponse = {
  success: boolean;
  message: string;
  session_id?: string;
  revoked_count?: number;
};

export type PortalHighlight = {
  label: string;
  value: string;
  note: string;
};

export type PortalHomeResponse = {
  portal_key: string;
  portal_title: string;
  allowed_subject_type: SubjectType;
  summary: string;
  highlights: PortalHighlight[];
};

export type StoredAuth = LoginResponse;

const STORAGE_KEY = "multi-subject-auth";

export function getBackendUrl() {
  return process.env.NEXT_PUBLIC_BACKEND_URL ?? "http://127.0.0.1:3001";
}

export function saveAuthSession(value: StoredAuth) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
}

export function loadAuthSession(): StoredAuth | null {
  const raw = localStorage.getItem(STORAGE_KEY);

  if (!raw) {
    return null;
  }

  try {
    return JSON.parse(raw) as StoredAuth;
  } catch {
    localStorage.removeItem(STORAGE_KEY);
    return null;
  }
}

export function clearAuthSession() {
  localStorage.removeItem(STORAGE_KEY);
}

export function updateStoredAuthSession(value: StoredAuth) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
}

export function subjectPortalPath(subjectType: SubjectType) {
  switch (subjectType) {
    case "MEMBER":
      return "/member";
    case "COMMUNITY_STAFF":
      return "/community-staff";
    case "PLATFORM_STAFF":
      return "/platform-staff";
  }
}
