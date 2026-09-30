// agent-notifier.opencode
// OpenCode V2 server plugin for Agent Notifier.
import { spawn } from 'node:child_process';

const AGENT_NOTIFIER = "__AGENT_NOTIFIER_BINARY__";

function getPath(value, path) {
    return path.split('.').reduce((current, key) => current?.[key], value);
}

function firstValue(roots, paths) {
    for (const root of roots) {
        for (const path of paths) {
            const value = getPath(root, path);
            if (value !== undefined && value !== null && value !== '')
                return value;
        }
    }
    return undefined;
}

function parseJson(value) {
    if (typeof value !== 'string')
        return value;
    try {
        return JSON.parse(value);
    } catch {
        return value;
    }
}

function textOf(value, depth = 0) {
    if (depth > 5 || value === undefined || value === null)
        return '';
    if (typeof value === 'string')
        return value;
    if (Array.isArray(value))
        return value.map((item) => textOf(item, depth + 1)).filter(Boolean).join(' ');
    if (typeof value !== 'object')
        return '';
    for (const key of ['text', 'content', 'message', 'summary', 'reason', 'error']) {
        const text = textOf(value[key], depth + 1);
        if (text)
            return text;
    }
    return '';
}

function eventData(event) {
    if (typeof event === 'string')
        return parseJson(event);
    return parseJson(event?.properties ?? event?.data ?? event?.details?.data ?? event?.details ?? {});
}

function normalize(event, directory) {
    const data = eventData(event);
    const session = firstValue([data, event], ['session', 'info', 'message.session', 'part.session']);
    const roots = [data, session, event];
    const type = String(firstValue([event, data], ['type', 'event']) ?? '').toLowerCase();
    const status = String(firstValue(roots, [
        'status.type', 'status.status', 'status', 'info.status', 'session.status',
    ]) ?? '').toLowerCase();
    const sessionId = firstValue(roots, [
        'session_id', 'sessionID', 'sessionId', 'info.id', 'session.id',
        'message.sessionID', 'part.sessionID',
    ]) ?? (type.startsWith('session.') ? firstValue(roots, ['id']) : undefined);
    if (typeof sessionId !== 'string' || !sessionId)
        return undefined;

    const role = String(firstValue(roots, [
        'role', 'type', 'message.role', 'message.type', 'info.role', 'info.type',
    ]) ?? '').toLowerCase();
    const state = String(firstValue(roots, ['state', 'part.state', 'message.state']) ?? '').toLowerCase();
    const finish = String(firstValue(roots, ['finish', 'info.finish', 'message.finish']) ?? '').toLowerCase();
    const completed = firstValue(roots, ['time.completed', 'info.time.completed', 'message.time.completed']);
    let hookEvent;
    if (type === 'session.created') {
        hookEvent = 'SessionStart';
    } else if (type === 'session.deleted' || type === 'session.ended') {
        hookEvent = 'SessionEnd';
    } else if (
        type === 'session.error' || type.endsWith('.error') || state === 'error' || finish === 'error'
    ) {
        hookEvent = 'StopFailure';
    } else if (
        type === 'permission.asked' || type === 'permission.requested' ||
        type === 'question.asked' || type === 'question.requested' ||
        type === 'form.asked' || type === 'form.requested'
    ) {
        hookEvent = 'PermissionRequest';
    } else if (
        type === 'session.completed' ||
        (type === 'session.status' && ['idle', 'completed', 'complete'].includes(status))
    ) {
        hookEvent = 'Stop';
    } else if (
        type === 'session.busy' || type === 'session.working' ||
        (type === 'session.status' && ['busy', 'working', 'retry'].includes(status))
    ) {
        hookEvent = 'Working';
    } else if (
        type === 'message.updated' && role === 'assistant' &&
        (Boolean(finish) || completed !== undefined)
    ) {
        hookEvent = 'Working';
    } else {
        return undefined;
    }

    const prompt = textOf(firstValue(roots, [
        'prompt', 'text', 'content', 'info.content', 'message', 'message.content',
        'part', 'error', 'reason',
    ]));
    const title = firstValue(roots, ['title', 'info.title', 'session.title']);
    const cwd = firstValue(roots, [
        'directory', 'cwd', 'info.directory', 'info.worktree',
        'location.directory', 'info.location.directory',
        'session.directory', 'session.cwd', 'session.location.directory',
    ]) ?? directory;
    return {
        hook_event_name: hookEvent,
        session_id: sessionId,
        cwd: typeof cwd === 'string' ? cwd : directory,
        title: typeof title === 'string' ? title : undefined,
        message: prompt || undefined,
        timestamp: new Date().toISOString(),
    };
}

function forward(payload) {
    try {
        const child = spawn(AGENT_NOTIFIER, ['hook', 'opencode'], {
            stdio: ['pipe', 'ignore', 'ignore'],
        });
        child.on('error', () => {});
        child.stdin?.on('error', () => {});
        child.stdin.end(JSON.stringify(payload));
    } catch {
        // Agent Notifier is informational and must never disrupt OpenCode.
    }
}

export default {
    id: 'agent-notifier.opencode',
    async setup(ctx) {
        const promptRegistration = await ctx.session.hook('prompt', (event) => {
            const prompt = event.prompt?.text;
            if (!prompt || !event.sessionID)
                return;
            forward({
                hook_event_name: 'UserPromptSubmit',
                session_id: event.sessionID,
                cwd: ctx.location.directory,
                prompt,
                timestamp: new Date().toISOString(),
            });
        });
        const controller = new AbortController();
        void (async () => {
            while (!controller.signal.aborted) {
                try {
                    for await (const event of ctx.event.subscribe({signal: controller.signal})) {
                        const payload = normalize(event, ctx.location.directory);
                        if (payload)
                            forward(payload);
                    }
                } catch {
                    // Reconnect below; Agent Notifier must never disrupt OpenCode.
                }
                if (!controller.signal.aborted)
                    await new Promise((resolve) => setTimeout(resolve, 1000));
            }
        })();
        return async () => {
            controller.abort();
            await promptRegistration.dispose();
        };
    },
};
