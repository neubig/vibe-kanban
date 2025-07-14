use async_trait::async_trait;
use command_group::{AsyncCommandGroup, AsyncGroupChild};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use uuid::Uuid;

use crate::{
    executor::{
        Executor, ExecutorError, NormalizedConversation, NormalizedEntry,
        NormalizedEntryType,
    },
    models::task::Task,
    utils::shell::get_shell_command,
};

/// An executor that uses OpenHands API to process tasks
pub struct OpenhandsExecutor;

/// An executor that resumes an OpenHands session
pub struct OpenhandsFollowupExecutor {
    pub conversation_id: String,
    pub prompt: String,
}

#[derive(Debug, Serialize)]
struct StartConversationRequest {
    initial_user_msg: String,
    repository: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StartConversationResponse {
    status: String,
    conversation_id: String,
}

#[derive(Debug, Deserialize)]
struct ConversationStatusResponse {
    conversation_id: String,
    title: String,
    created_at: String,
    last_updated_at: String,
    status: String,
    selected_repository: Option<String>,
    trigger: String,
}

#[async_trait]
impl Executor for OpenhandsExecutor {
    async fn spawn(
        &self,
        pool: &sqlx::SqlitePool,
        task_id: Uuid,
        worktree_path: &str,
    ) -> Result<AsyncGroupChild, ExecutorError> {
        // Get the task to fetch its description
        let task = Task::find_by_id(pool, task_id)
            .await?
            .ok_or(ExecutorError::TaskNotFound)?;

        let prompt = if let Some(task_description) = task.description {
            format!(
                r#"project_id: {}
            
Task title: {}
Task description: {}"#,
                task.project_id, task.title, task_description
            )
        } else {
            format!(
                r#"project_id: {}
            
Task title: {}"#,
                task.project_id, task.title
            )
        };

        // Create a script that will handle the OpenHands API interaction
        let script_content = format!(
            r#"#!/bin/bash
set -e

# Check if OPENHANDS_API_KEY is set
if [ -z "$OPENHANDS_API_KEY" ]; then
    echo "Error: OPENHANDS_API_KEY environment variable is not set"
    exit 1
fi

# Start conversation
CONVERSATION_RESPONSE=$(curl -s -X POST "https://app.all-hands.dev/api/conversations" \
  -H "Authorization: Bearer $OPENHANDS_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{{
    "initial_user_msg": "{}",
    "repository": null
  }}')

# Extract conversation ID
CONVERSATION_ID=$(echo "$CONVERSATION_RESPONSE" | grep -o '"conversation_id":"[^"]*"' | cut -d'"' -f4)

if [ -z "$CONVERSATION_ID" ]; then
    echo "Error: Failed to start conversation"
    echo "$CONVERSATION_RESPONSE"
    exit 1
fi

echo "Started OpenHands conversation: $CONVERSATION_ID"

# Poll for status updates
while true; do
    STATUS_RESPONSE=$(curl -s -X GET "https://app.all-hands.dev/api/conversations/$CONVERSATION_ID" \
      -H "Authorization: Bearer $OPENHANDS_API_KEY")
    
    STATUS=$(echo "$STATUS_RESPONSE" | grep -o '"status":"[^"]*"' | cut -d'"' -f4)
    
    echo "Status: $STATUS"
    
    if [ "$STATUS" = "COMPLETED" ] || [ "$STATUS" = "FAILED" ] || [ "$STATUS" = "CANCELLED" ]; then
        echo "Conversation finished with status: $STATUS"
        break
    fi
    
    sleep 5
done

echo "OpenHands conversation completed"
"#,
            prompt.replace('"', r#"\""#).replace('\n', r#"\n"#)
        );

        // Write the script to a temporary file
        let script_path = format!("/tmp/openhands_task_{}.sh", task_id);
        tokio::fs::write(&script_path, script_content).await.map_err(|e| {
            ExecutorError::ContextCollectionFailed(format!("Failed to write OpenHands script: {}", e))
        })?;

        // Make the script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&script_path).await.map_err(|e| {
                ExecutorError::ContextCollectionFailed(format!("Failed to get script metadata: {}", e))
            })?.permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(&script_path, perms).await.map_err(|e| {
                ExecutorError::ContextCollectionFailed(format!("Failed to set script permissions: {}", e))
            })?;
        }

        // Use shell command for cross-platform compatibility
        let (shell_cmd, shell_arg) = get_shell_command();

        let mut command = Command::new(shell_cmd);
        command
            .kill_on_drop(true)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .current_dir(worktree_path)
            .arg(shell_arg)
            .arg(&script_path)
            .env("NODE_NO_WARNINGS", "1");

        let child = command
            .group_spawn() // Create new process group so we can kill entire tree
            .map_err(|e| {
                crate::executor::SpawnContext::from_command(&command, "OpenHands")
                    .with_task(task_id, Some(task.title.clone()))
                    .with_context("OpenHands API execution for new task")
                    .spawn_error(e)
            })?;

        Ok(child)
    }

    fn normalize_logs(
        &self,
        logs: &str,
        _worktree_path: &str,
    ) -> Result<NormalizedConversation, String> {
        let mut entries = Vec::new();
        let mut conversation_id = None;

        for line in logs.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Extract conversation ID
            if trimmed.starts_with("Started OpenHands conversation:") {
                if let Some(id) = trimmed.split(':').nth(1) {
                    conversation_id = Some(id.trim().to_string());
                    entries.push(NormalizedEntry {
                        timestamp: None,
                        entry_type: NormalizedEntryType::SystemMessage,
                        content: format!("Started OpenHands conversation: {}", id.trim()),
                        metadata: None,
                    });
                }
                continue;
            }

            // Parse status updates
            if trimmed.starts_with("Status:") {
                if let Some(status) = trimmed.split(':').nth(1) {
                    let status = status.trim();
                    entries.push(NormalizedEntry {
                        timestamp: None,
                        entry_type: NormalizedEntryType::SystemMessage,
                        content: format!("Conversation status: {}", status),
                        metadata: None,
                    });
                }
                continue;
            }

            // Parse completion message
            if trimmed.starts_with("Conversation finished with status:") {
                entries.push(NormalizedEntry {
                    timestamp: None,
                    entry_type: NormalizedEntryType::SystemMessage,
                    content: trimmed.to_string(),
                    metadata: None,
                });
                continue;
            }

            // Parse final completion
            if trimmed == "OpenHands conversation completed" {
                entries.push(NormalizedEntry {
                    timestamp: None,
                    entry_type: NormalizedEntryType::SystemMessage,
                    content: "OpenHands conversation completed successfully".to_string(),
                    metadata: None,
                });
                continue;
            }

            // Handle error messages
            if trimmed.starts_with("Error:") {
                entries.push(NormalizedEntry {
                    timestamp: None,
                    entry_type: NormalizedEntryType::ErrorMessage,
                    content: trimmed.to_string(),
                    metadata: None,
                });
                continue;
            }

            // Add any other output as system messages
            entries.push(NormalizedEntry {
                timestamp: None,
                entry_type: NormalizedEntryType::SystemMessage,
                content: trimmed.to_string(),
                metadata: None,
            });
        }

        Ok(NormalizedConversation {
            entries,
            session_id: conversation_id,
            executor_type: "openhands".to_string(),
            prompt: None,
            summary: None,
        })
    }
}

#[async_trait]
impl Executor for OpenhandsFollowupExecutor {
    async fn spawn(
        &self,
        pool: &sqlx::SqlitePool,
        task_id: Uuid,
        worktree_path: &str,
    ) -> Result<AsyncGroupChild, ExecutorError> {
        // Get the task to fetch its description
        let _task = Task::find_by_id(pool, task_id)
            .await?
            .ok_or(ExecutorError::TaskNotFound)?;

        // For followup, we would need to implement conversation continuation
        // For now, we'll create a new conversation with the followup prompt
        let main_executor = OpenhandsExecutor;
        main_executor.spawn(pool, task_id, worktree_path).await
    }

    fn normalize_logs(
        &self,
        logs: &str,
        worktree_path: &str,
    ) -> Result<NormalizedConversation, String> {
        // Reuse the same logic as the main OpenhandsExecutor
        let main_executor = OpenhandsExecutor;
        main_executor.normalize_logs(logs, worktree_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_logs_basic() {
        let executor = OpenhandsExecutor;
        let logs = r#"Starting OpenHands API call...
API Response: {"status": "success", "message": "Task completed"}
Task execution completed successfully"#;

        let result = executor.normalize_logs(logs, "/tmp/test-worktree").unwrap();

        assert_eq!(result.entries.len(), 3);
        assert_eq!(result.entries[0].entry_type, NormalizedEntryType::SystemMessage);
        assert_eq!(result.entries[0].content, "Starting OpenHands API call...");
        
        assert_eq!(result.entries[1].entry_type, NormalizedEntryType::SystemMessage);
        assert_eq!(result.entries[1].content, "API Response: {\"status\": \"success\", \"message\": \"Task completed\"}");
        
        assert_eq!(result.entries[2].entry_type, NormalizedEntryType::SystemMessage);
        assert_eq!(result.entries[2].content, "Task execution completed successfully");
    }

    #[test]
    fn test_normalize_logs_empty() {
        let executor = OpenhandsExecutor;
        let logs = "";

        let result = executor.normalize_logs(logs, "/tmp/test-worktree").unwrap();

        assert_eq!(result.entries.len(), 0);
    }

    #[test]
    fn test_normalize_logs_single_line() {
        let executor = OpenhandsExecutor;
        let logs = "Single log line";

        let result = executor.normalize_logs(logs, "/tmp/test-worktree").unwrap();

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].entry_type, NormalizedEntryType::SystemMessage);
        assert_eq!(result.entries[0].content, "Single log line");
    }

    #[test]
    fn test_normalize_logs_with_empty_lines() {
        let executor = OpenhandsExecutor;
        let logs = r#"First line

Second line after empty line

Third line"#;

        let result = executor.normalize_logs(logs, "/tmp/test-worktree").unwrap();

        // Empty lines should be filtered out
        assert_eq!(result.entries.len(), 3);
        assert_eq!(result.entries[0].content, "First line");
        assert_eq!(result.entries[1].content, "Second line after empty line");
        assert_eq!(result.entries[2].content, "Third line");
    }

    #[test]
    fn test_followup_executor_normalize_logs() {
        let executor = OpenhandsFollowupExecutor {
            conversation_id: "test-conv-123".to_string(),
            prompt: "Continue the task".to_string(),
        };
        
        let logs = "Followup task execution log";

        let result = executor.normalize_logs(logs, "/tmp/test-worktree").unwrap();

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].entry_type, NormalizedEntryType::SystemMessage);
        assert_eq!(result.entries[0].content, "Followup task execution log");
    }
}