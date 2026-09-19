/**
 * Prumo - OpenCode Native Plugin
 * Version: 0.5.0
 * 
 * Provides native harness integration between OpenCode and Prumo:
 * - Pre-tool validation (tool guards)
 * - Session lifecycle hooks (start, end, tool events)
 * - Lean Progressive Context injection
 * - Command routing to Prumo CLI
 */

import * as fs from 'fs';
import * as path from 'path';

export interface PrumoPluginConfig {
  prumoHome?: string;
  projectRoot?: string;
  enforceToolGuards?: boolean;
}

export interface ToolGuardResult {
  allowed: boolean;
  reason?: string;
}

// Tool Guard: validates tool execution before execution (pre-tool block)
export function validateToolExecution(toolName: string, params: Record<string, any>, policyPath?: string): ToolGuardResult {
  const dangerousCommands = [
    'rm -rf /',
    'rm -rf /*',
    'mkfs',
    ':(){ :|:& };:',
    'dd if=/dev/zero'
  ];

  if (toolName === 'bash' || toolName === 'execute_command' || toolName === 'run_command') {
    const cmd = String(params.command || params.cmd || params.CommandLine || '');
    for (const dangerous of dangerousCommands) {
      if (cmd.includes(dangerous)) {
        return {
          allowed: false,
          reason: 'Prumo Tool Guard blocked execution of dangerous command: ' + dangerous
        };
      }
    }
  }

  if (toolName === 'write_file' || toolName === 'write_to_file') {
    const target = String(params.path || params.TargetFile || '');
    if (target.includes('.git/') || target.includes('.prumo/credentials')) {
      return {
        allowed: false,
        reason: 'Prumo Tool Guard protected sensitive path: ' + target
      };
    }
  }

  return { allowed: true };
}

// Session Lifecycle Hook: Start
export function onSessionStart(sessionID: string, projectRoot: string): { contextPrompt: string } {
  const entrypointPath = path.join(projectRoot, 'ENTRYPOINT.md');
  let entrypointText = '';
  if (fs.existsSync(entrypointPath)) {
    entrypointText = fs.readFileSync(entrypointPath, 'utf8');
  }

  return {
    contextPrompt: [
      '[Prumo v0.5] Native OpenCode Harness Active',
      'Follow Lean Progressive Context: smallest sufficient context, pointer over payload.',
      'Treat Prumo as an external CLI utility available in PATH (\'prumo\'). Use \'prumo <command>\' for project operations. Do not inspect internal framework development source code.',
      'Active Entrypoint:',
      entrypointText
    ].join('\n\n')
  };
}

// Session Lifecycle Hook: End
export function onSessionEnd(sessionID: string, projectRoot: string, summary?: string): void {
  const expDir = path.join(projectRoot, '.prumo', 'experience');
  if (fs.existsSync(expDir)) {
    const eventFile = path.join(expDir, 'session-' + sessionID + '.json');
    const payload = {
      session_id: sessionID,
      event_type: 'session_completed',
      summary: summary || 'OpenCode session completed',
      timestamp: new Date().toISOString()
    };
    fs.writeFileSync(eventFile, JSON.stringify(payload, null, 2));
  }
}
