#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;
use hyperlight_common::flatbuffer_wrappers::function_call::FunctionCall;
use hyperlight_common::flatbuffer_wrappers::function_types::{
    ParameterType, ParameterValue, ReturnType,
};
use hyperlight_common::flatbuffer_wrappers::guest_error::ErrorCode;
use hyperlight_common::flatbuffer_wrappers::util::get_flatbuffer_result;
use hyperlight_guest::error::{HyperlightGuestError, Result};
use hyperlight_guest_bin::guest_function::definition::GuestFunctionDefinition;
use hyperlight_guest_bin::guest_function::register::register_function;
use hyperlight_guest_bin::host_comm::call_host_function;
use tracing::{Span, instrument};

/// Main entry point for the hyperlight guest
/// Registers all available guest functions
#[no_mangle]
#[instrument(skip_all, parent = Span::current(), level = "Trace")]
pub extern "C" fn hyperlight_main() {
    // Register the main agent execution function
    let execute_agent_def = GuestFunctionDefinition::new(
        "ExecuteAgent".to_string(),
        Vec::from(&[
            ParameterType::String,  // prompt
            ParameterType::String,  // mcp_server_url
        ]),
        ReturnType::String,
        execute_agent as usize,
    );
    register_function(execute_agent_def);

    // Register MCP tool call function
    let call_mcp_tool_def = GuestFunctionDefinition::new(
        "CallMCPTool".to_string(),
        Vec::from(&[
            ParameterType::String,  // tool_name
            ParameterType::String,  // arguments (JSON)
        ]),
        ReturnType::String,
        call_mcp_tool as usize,
    );
    register_function(call_mcp_tool_def);
}

/// Main agent execution function
/// This function receives a prompt and executes the agent logic
/// All network I/O is delegated to host functions
fn execute_agent(function_call: &FunctionCall) -> Result<Vec<u8>> {
    let params = function_call.parameters.as_ref()
        .ok_or_else(|| HyperlightGuestError::new(
            ErrorCode::GuestFunctionParameterTypeMismatch,
            "Missing parameters".to_string(),
        ))?;

    let prompt = match &params[0] {
        ParameterValue::String(s) => s,
        _ => return Err(HyperlightGuestError::new(
            ErrorCode::GuestFunctionParameterTypeMismatch,
            "First parameter must be string (prompt)".to_string(),
        )),
    };

    let mcp_server_url = match &params[1] {
        ParameterValue::String(s) => s,
        _ => return Err(HyperlightGuestError::new(
            ErrorCode::GuestFunctionParameterTypeMismatch,
            "Second parameter must be string (mcp_server_url)".to_string(),
        )),
    };

    // Agent logic implementation
    // 1. Initialize connection to MCP server (through host)
    call_host_function::<()>(
        "InitializeMCPConnection",
        Some(Vec::from(&[ParameterValue::String(mcp_server_url.clone())])),
        ReturnType::Void,
    )?;

    // 2. Get available tools from MCP server
    let tools_json = call_host_function::<String>(
        "GetMCPTools",
        None,
        ReturnType::String,
    )?;

    // 3. Process the prompt and determine which tools to use
    let response = process_agent_request(prompt, &tools_json)?;

    Ok(get_flatbuffer_result(&*response))
}

/// Process an agent request with the given prompt and available tools
fn process_agent_request(prompt: &str, tools_json: &str) -> Result<String> {
    // Simple agent logic:
    // 1. Analyze the prompt
    // 2. Determine which tools to call
    // 3. Execute tool calls through the host
    // 4. Format and return the response

    // For now, return a simple response that demonstrates the agent is working
    let response = format!(
        "Agent processed prompt: '{}'\nAvailable tools: {}\n\nAgent is running securely in Hyperlight guest!",
        prompt,
        tools_json
    );

    Ok(response)
}

/// Call an MCP tool through the host
/// The host enforces that only the configured MCP server can be accessed
fn call_mcp_tool(function_call: &FunctionCall) -> Result<Vec<u8>> {
    let params = function_call.parameters.as_ref()
        .ok_or_else(|| HyperlightGuestError::new(
            ErrorCode::GuestFunctionParameterTypeMismatch,
            "Missing parameters".to_string(),
        ))?;

    let tool_name = match &params[0] {
        ParameterValue::String(s) => s,
        _ => return Err(HyperlightGuestError::new(
            ErrorCode::GuestFunctionParameterTypeMismatch,
            "First parameter must be string (tool_name)".to_string(),
        )),
    };

    let arguments_json = match &params[1] {
        ParameterValue::String(s) => s,
        _ => return Err(HyperlightGuestError::new(
            ErrorCode::GuestFunctionParameterTypeMismatch,
            "Second parameter must be string (arguments)".to_string(),
        )),
    };

    // Call the host function to execute the MCP tool
    // The host will make the actual HTTP request to the MCP server
    let result = call_host_function::<String>(
        "ExecuteMCPTool",
        Some(Vec::from(&[
            ParameterValue::String(tool_name.clone()),
            ParameterValue::String(arguments_json.clone()),
        ])),
        ReturnType::String,
    )?;

    Ok(get_flatbuffer_result(&*result))
}
