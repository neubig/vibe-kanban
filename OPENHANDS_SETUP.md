# OpenHands API Integration

This document explains how to set up and use the OpenHands API as a coding agent in vibe-kanban.

## Prerequisites

1. **OpenHands API Key**: You need an API key from [OpenHands Cloud](https://app.all-hands.dev/)
2. **Environment Setup**: The API key must be available as an environment variable

## Setup

### 1. Get Your API Key

1. Visit [OpenHands Cloud](https://app.all-hands.dev/)
2. Sign up or log in to your account
3. Navigate to your API settings to generate an API key

### 2. Set Environment Variable

Set the `OPENHANDS_API_KEY` environment variable:

**Linux/macOS:**
```bash
export OPENHANDS_API_KEY="your-api-key-here"
```

**Windows (Command Prompt):**
```cmd
set OPENHANDS_API_KEY=your-api-key-here
```

**Windows (PowerShell):**
```powershell
$env:OPENHANDS_API_KEY="your-api-key-here"
```

### 3. Start vibe-kanban

Start vibe-kanban with the environment variable set:

```bash
# Make sure the environment variable is set
echo $OPENHANDS_API_KEY

# Start the application
cargo run
```

## Usage

1. **Create a Project**: Set up your project in vibe-kanban as usual
2. **Select OpenHands Executor**: When creating or configuring tasks, select "OpenHands" as the executor type
3. **Create Tasks**: Create tasks with clear descriptions - these will be sent to the OpenHands API
4. **Monitor Execution**: Watch the task execution in real-time through the vibe-kanban interface

## Features

### New Task Execution
- Creates a new conversation with the OpenHands API
- Sends project context (project ID, task title, description)
- Streams execution logs back to vibe-kanban

### Followup Tasks
- Continues existing conversations
- Maintains context from previous interactions
- Supports iterative development workflows

### Error Handling
- Graceful handling of API errors
- Clear error messages in the UI
- Automatic retry mechanisms where appropriate

## API Integration Details

The OpenHands executor integrates with the [OpenHands Cloud API](https://docs.all-hands.dev/usage/cloud/cloud-api) using:

- **Endpoint**: `https://api.all-hands.dev/v1/conversations`
- **Authentication**: Bearer token using your API key
- **Request Format**: JSON with task context and instructions
- **Response**: Streaming conversation updates

## Troubleshooting

### Common Issues

1. **"API key not found" error**
   - Ensure `OPENHANDS_API_KEY` environment variable is set
   - Verify the API key is valid and not expired

2. **Connection errors**
   - Check your internet connection
   - Verify the OpenHands API is accessible from your network

3. **Task execution fails**
   - Check the task description is clear and actionable
   - Review the execution logs for specific error messages

### Debug Mode

For debugging, you can check the generated shell scripts in `/tmp/openhands_task_*.sh` to see the exact API calls being made.

## Security Notes

- **API Key Security**: Never commit your API key to version control
- **Environment Variables**: Use secure methods to set environment variables in production
- **Network Security**: The executor makes HTTPS requests to the OpenHands API

## Limitations

- **MCP Support**: OpenHands executor doesn't support MCP (Model Context Protocol) configuration
- **Offline Mode**: Requires internet connection to function
- **Rate Limits**: Subject to OpenHands API rate limits

## Support

For issues related to:
- **vibe-kanban integration**: Create an issue in the vibe-kanban repository
- **OpenHands API**: Refer to the [OpenHands documentation](https://docs.all-hands.dev/) or support channels