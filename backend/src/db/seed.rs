use anyhow::{Context, anyhow};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use sqlx::SqlitePool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::auth::{SubjectType, normalize_identifier};

pub async fn run(pool: &SqlitePool) -> anyhow::Result<()> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .context("failed to format seed timestamp")?;

    let definitions = [
        SeedSubject {
            id: "subject_member_001",
            subject_type: SubjectType::Member,
            display_name: "Member Account",
            identifiers: &[
                ("EMAIL", "member@example.com"),
                ("PHONE", "13800000001"),
                ("MEMBER_NO", "member001"),
            ],
            otp_channels: &[("EMAIL", "member@example.com"), ("PHONE", "13800000001")],
            password: "Password123!",
        },
        SeedSubject {
            id: "subject_community_staff_001",
            subject_type: SubjectType::CommunityStaff,
            display_name: "Community Staff Account",
            identifiers: &[
                ("EMAIL", "community.staff@example.com"),
                ("STAFF_NO", "cstaff001"),
            ],
            otp_channels: &[("EMAIL", "community.staff@example.com")],
            password: "Password123!",
        },
        SeedSubject {
            id: "subject_platform_staff_001",
            subject_type: SubjectType::PlatformStaff,
            display_name: "Platform Staff Account",
            identifiers: &[
                ("EMAIL", "platform.staff@example.com"),
                ("STAFF_NO", "pstaff001"),
            ],
            otp_channels: &[("EMAIL", "platform.staff@example.com")],
            password: "Password123!",
        },
    ];

    for definition in definitions {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO subjects (id, subject_type, status, display_name, created_at, updated_at)
            VALUES (?1, ?2, 'ACTIVE', ?3, ?4, ?4)
            "#,
        )
        .bind(definition.id)
        .bind(definition.subject_type.as_str())
        .bind(definition.display_name)
        .bind(&now)
        .execute(pool)
        .await
        .with_context(|| format!("failed to seed subject {}", definition.id))?;

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO credentials (id, subject_id, credential_type, status, created_at, updated_at)
            VALUES (?1, ?2, 'PASSWORD', 'ACTIVE', ?3, ?3)
            "#,
        )
        .bind(format!("cred_password_{}", definition.id))
        .bind(definition.id)
        .bind(&now)
        .execute(pool)
        .await
        .with_context(|| format!("failed to seed credential row for {}", definition.id))?;

        for (kind, value) in definition.identifiers {
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO subject_identifiers (id, subject_id, identifier_kind, identifier_value, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(definition.id)
            .bind(kind)
            .bind(normalize_identifier(value))
            .bind(&now)
            .execute(pool)
            .await
            .with_context(|| format!("failed to seed identifier {value}"))?;
        }

        for (channel_type, channel_value) in definition.otp_channels {
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO otp_identities (id, subject_id, channel_type, channel_value, is_enabled, created_at)
                VALUES (?1, ?2, ?3, ?4, 1, ?5)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(definition.id)
            .bind(channel_type)
            .bind(normalize_identifier(channel_value))
            .bind(&now)
            .execute(pool)
            .await
            .with_context(|| format!("failed to seed otp identity {channel_value}"))?;
        }

        let password_hash = hash_password(definition.password)?;
        sqlx::query(
            r#"
            INSERT INTO password_credentials (subject_id, password_hash, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(subject_id) DO UPDATE SET
                password_hash = excluded.password_hash,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(definition.id)
        .bind(password_hash)
        .bind(&now)
        .execute(pool)
        .await
        .with_context(|| format!("failed to seed password credential {}", definition.id))?;
    }

    Ok(())
}

fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt_source = Uuid::new_v4();
    let salt = SaltString::encode_b64(salt_source.as_bytes())
        .map_err(|error| anyhow!("failed to build password salt: {error}"))?;
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| anyhow!("failed to hash password during seed: {error}"))?;

    Ok(hash.to_string())
}

struct SeedSubject {
    id: &'static str,
    subject_type: SubjectType,
    display_name: &'static str,
    identifiers: &'static [(&'static str, &'static str)],
    otp_channels: &'static [(&'static str, &'static str)],
    password: &'static str,
}
