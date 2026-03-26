"use client";

export type CredentialDescriptorJson = {
  type: string;
  id: string;
  transports?: string[];
};

export type PasskeyRegisterOptionsResponse = {
  challenge_id: string;
  expires_at: string;
  public_key: {
    rp: {
      id: string;
      name: string;
    };
    user: {
      id: string;
      name: string;
      display_name: string;
    };
    challenge: string;
    timeout: number;
    attestation: AttestationConveyancePreference;
    exclude_credentials: CredentialDescriptorJson[];
    authenticator_selection: {
      resident_key: ResidentKeyRequirement;
      user_verification: UserVerificationRequirement;
      authenticator_attachment?: AuthenticatorAttachment;
    };
    pub_key_cred_params: Array<{
      type: "public-key";
      alg: number;
    }>;
  };
};

export type PasskeyRegisterVerifyResponse = {
  success: boolean;
  message: string;
  credential_id: string;
  authenticator_label: string;
};

export type PasskeyLoginOptionsResponse = {
  challenge_id: string;
  expires_at: string;
  credential_count: number;
  public_key: {
    challenge: string;
    timeout: number;
    rp_id: string;
    allow_credentials: CredentialDescriptorJson[];
    user_verification: UserVerificationRequirement;
  };
};

type EncodedRegistrationCredential = {
  id: string;
  raw_id: string;
  type: string;
  authenticator_attachment?: string;
  response: {
    client_data_json: string;
    attestation_object: string;
    transports: string[];
  };
};

type EncodedAuthenticationCredential = {
  id: string;
  raw_id: string;
  type: string;
  response: {
    client_data_json: string;
    authenticator_data: string;
    signature: string;
    user_handle?: string;
  };
};

export function isPasskeySupported() {
  return (
    typeof window !== "undefined" &&
    "PublicKeyCredential" in window &&
    window.isSecureContext
  );
}

export function buildRegistrationRequest(
  options: PasskeyRegisterOptionsResponse["public_key"],
): PublicKeyCredentialCreationOptions {
  return {
    rp: options.rp,
    user: {
      id: new TextEncoder().encode(options.user.id),
      name: options.user.name,
      displayName: options.user.display_name,
    },
    challenge: base64UrlToBytes(options.challenge),
    timeout: options.timeout,
    attestation: options.attestation,
    authenticatorSelection: {
      residentKey: options.authenticator_selection.resident_key,
      userVerification: options.authenticator_selection.user_verification,
      authenticatorAttachment:
        options.authenticator_selection.authenticator_attachment,
    },
    excludeCredentials: options.exclude_credentials.map((item) => ({
      type: "public-key",
      id: base64UrlToBytes(item.id),
      transports: normalizeTransports(item.transports),
    })),
    pubKeyCredParams: options.pub_key_cred_params.map((item) => ({
      type: "public-key",
      alg: item.alg,
    })),
  };
}

export function buildAuthenticationRequest(
  options: PasskeyLoginOptionsResponse["public_key"],
): PublicKeyCredentialRequestOptions {
  return {
    challenge: base64UrlToBytes(options.challenge),
    timeout: options.timeout,
    rpId: options.rp_id,
    allowCredentials: options.allow_credentials.map((item) => ({
      type: "public-key",
      id: base64UrlToBytes(item.id),
      transports: normalizeTransports(item.transports),
    })),
    userVerification: options.user_verification,
  };
}

export function serializeRegistrationCredential(
  credential: PublicKeyCredential,
): EncodedRegistrationCredential {
  const response = credential.response as AuthenticatorAttestationResponse;
  const transports =
    typeof response.getTransports === "function" ? response.getTransports() : [];

  return {
    id: credential.id,
    raw_id: bytesToBase64Url(credential.rawId),
    type: credential.type,
    authenticator_attachment: credential.authenticatorAttachment ?? undefined,
    response: {
      client_data_json: bytesToBase64Url(response.clientDataJSON),
      attestation_object: bytesToBase64Url(response.attestationObject),
      transports,
    },
  };
}

export function serializeAuthenticationCredential(
  credential: PublicKeyCredential,
): EncodedAuthenticationCredential {
  const response = credential.response as AuthenticatorAssertionResponse;

  return {
    id: credential.id,
    raw_id: bytesToBase64Url(credential.rawId),
    type: credential.type,
    response: {
      client_data_json: bytesToBase64Url(response.clientDataJSON),
      authenticator_data: bytesToBase64Url(response.authenticatorData),
      signature: bytesToBase64Url(response.signature),
      user_handle: response.userHandle
        ? bytesToBase64Url(response.userHandle)
        : undefined,
    },
  };
}

export function describePasskeyError(error: unknown) {
  if (error instanceof DOMException) {
    if (error.name === "NotAllowedError") {
      return "浏览器取消了 Passkey 操作，或当前设备没有可用凭证。";
    }

    if (error.name === "InvalidStateError") {
      return "该 Passkey 可能已经绑定过当前主体。";
    }

    if (error.name === "SecurityError") {
      return "当前页面不是可用的安全上下文，浏览器拒绝了 Passkey。";
    }

    return error.message || "Passkey 操作失败。";
  }

  return "Passkey 操作失败。";
}

function base64UrlToBytes(value: string) {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padding = "=".repeat((4 - (normalized.length % 4 || 4)) % 4);
  const base64 = normalized + padding;
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);

  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }

  return bytes.buffer;
}

function bytesToBase64Url(value: ArrayBuffer | ArrayBufferView) {
  const bytes =
    value instanceof ArrayBuffer
      ? new Uint8Array(value)
      : new Uint8Array(value.buffer, value.byteOffset, value.byteLength);

  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }

  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function normalizeTransports(
  value: string[] | undefined,
): AuthenticatorTransport[] | undefined {
  if (!value?.length) {
    return undefined;
  }

  const allowed = new Set<AuthenticatorTransport>([
    "ble",
    "hybrid",
    "internal",
    "nfc",
    "usb",
  ]);

  return value.filter((item): item is AuthenticatorTransport =>
    allowed.has(item as AuthenticatorTransport),
  );
}
