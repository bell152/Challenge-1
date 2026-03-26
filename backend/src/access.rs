use crate::{
    auth::{SubjectResponse, SubjectType},
    error::ApiError,
};

pub fn require_subject_type(
    subject: &SubjectResponse,
    required: SubjectType,
) -> Result<(), ApiError> {
    if subject.subject_type == required {
        return Ok(());
    }

    Err(ApiError::forbidden(
        "PORTAL_FORBIDDEN",
        format!(
            "{} subject cannot access {} portal",
            subject.subject_type.as_str(),
            required.as_str()
        ),
    ))
}

