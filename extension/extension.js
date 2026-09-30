import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Pango from 'gi://Pango';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

const IFACE = `<node><interface name="io.github.mmmohebi.AgentNotifier1"><method name="ListSessions"><arg direction="in" type="x" name="since"/><arg direction="out" type="s" name="json"/></method><method name="SetSessionNotifications"><arg direction="in" type="s" name="session_id"/><arg direction="in" type="b" name="enabled"/><arg direction="out" type="s" name="json"/></method><method name="SetAllSessionNotifications"><arg direction="in" type="b" name="enabled"/><arg direction="out" type="s" name="json"/></method><method name="SetSessionHidden"><arg direction="in" type="s" name="session_id"/><arg direction="in" type="b" name="hidden"/><arg direction="out" type="s" name="json"/></method><signal name="SessionsChanged"><arg type="t" name="revision"/></signal></interface></node>`;
const Proxy = Gio.DBusProxy.makeProxyWrapper(IFACE);

const MENU_TEXT_WIDTH = 390;
const MAX_PANEL_CHIPS = 4;
const STATE_META = {
    working: {emoji: '⚡', label: 'Working', css: 'working'},
    needs_attention: {emoji: '🔴', label: 'Attention', css: 'needs-attention'},
    completed: {emoji: '🟢', label: 'Done', css: 'completed'},
    failed: {emoji: '❌', label: 'Failed', css: 'failed'},
    ended: {emoji: '⚪', label: 'Ended', css: 'ended'},
    unknown: {emoji: '❔', label: 'Unknown', css: 'unknown'},
};

class ScrollableSessionSection extends PopupMenu.PopupMenuSection {
    constructor() {
        super();
        this.actor = new St.ScrollView({
            style_class: 'agent-notifier-scroll',
            hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC,
            overlay_scrollbars: true,
            enable_mouse_scrolling: true,
        });
        this.actor.add_child(this.box);
        this.actor._delegate = this;
    }
}

const Indicator = GObject.registerClass(class Indicator extends PanelMenu.Button {
    _init(extension) {
        super._init(0.0, 'Agent Notifier');
        this._destroyed = false;
        this._lastError = null;
        this._extension = extension;
        this._settings = extension.getSettings();
        this._settingsChanged = this._settings.connect('changed', () => this._refresh());
        this._showHidden = false;
        this._workingDots = [];
        this._panelBox = new St.BoxLayout({
            style_class: 'panel-status-menu-box agent-notifier-panel',
        });
        this.add_child(this._panelBox);

        this._proxy = new Proxy(
            Gio.DBus.session,
            'io.github.mmmohebi.AgentNotifier',
            '/io/github/mmmohebi/AgentNotifier',
            (proxy, error) => {
                if (this._destroyed)
                    return;
                if (error) {
                    this._logError(error.message);
                    this._showUnavailable();
                    return;
                }

                this._signal = proxy.connectSignal('SessionsChanged', () => this._refresh());
                this._refresh();
            }
        );

        this._menuSignal = this.menu.connect('open-state-changed', (_menu, open) => {
            if (open)
                this._refresh();
        });
        this._refreshTimer = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT,
            this._settings.get_uint('refresh-seconds'),
            () => {
                this._refresh();
                return GLib.SOURCE_CONTINUE;
            }
        );
        this._blinkTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 650, () => {
            for (const dot of this._workingDots)
                dot.opacity = dot.opacity === 255 ? 70 : 255;
            return GLib.SOURCE_CONTINUE;
        });
    }

    _showUnavailable() {
        this.menu.removeAll();
        const card = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        card.add_style_class_name('agent-notifier-unavailable');
        const box = new St.BoxLayout({vertical: true});
        box.add_child(new St.Label({
            text: 'Companion service unavailable',
            style_class: 'agent-notifier-card-title agent-notifier-failed-text',
        }));
        box.add_child(new St.Label({
            text: 'Open Settings to check installation and service health.',
            style_class: 'agent-notifier-card-preview',
        }));
        card.add_child(box);
        this.menu.addMenuItem(card);
        this.menu.addMenuItem(this._settingsItem());

        this._renderPanel([]);
        this._panelBox.add_child(new St.Label({
            text: '!',
            style_class: 'agent-notifier-error',
        }));
    }

    _refresh() {
        if (this._destroyed || !this._proxy)
            return;

        const historySeconds = this._settings.get_uint('history-hours') * 60 * 60;
        const since = Math.floor(Date.now() / 1000) - historySeconds;
        this._proxy.ListSessionsRemote(since, (result, error) => {
            if (this._destroyed)
                return;
            if (error) {
                this._logError(error.message);
                this._showUnavailable();
                return;
            }

            let sessions;
            try {
                sessions = JSON.parse(result[0]);
            } catch (parseError) {
                this._logError(`Invalid service response: ${parseError.message}`);
                this._showUnavailable();
                return;
            }

            if (!Array.isArray(sessions)) {
                this._showUnavailable();
                return;
            }

            this._lastError = null;
            this._renderPanel(sessions);
            this._renderMenu(sessions);
        });
    }

    _renderPanel(sessions) {
        this._panelBox.destroy_all_children();
        this._workingDots = [];

        const now = Date.now() / 1000;
        const recentlyDoneSeconds =
            this._settings.get_uint('topbar-completed-minutes') * 60;
        const visibleSessions = sessions.filter(session => !session.hidden);
        const visible = visibleSessions.filter(session => {
            const age = now - this._timestamp(session.updated_at);
            if (['completed', 'failed'].includes(session.state))
                return age <= recentlyDoneSeconds;
            return ['working', 'needs_attention'].includes(session.state);
        });

        if (!visible.length) {
            this._panelBox.add_child(new St.Icon({
                icon_name: 'system-run-symbolic',
                style_class: 'system-status-icon agent-notifier-idle',
            }));
            this._panelBox.add_child(new St.Label({
                text: String(visibleSessions.length),
                style_class: 'agent-notifier-recent-count',
                y_align: Clutter.ActorAlign.CENTER,
            }));
            return;
        }

        for (const session of visible.slice(0, MAX_PANEL_CHIPS)) {
            const state = this._state(session.state);
            const chip = new St.BoxLayout({
                style_class: `agent-notifier-chip agent-notifier-chip-${state.css}`,
            });
            const dot = new St.Widget({
                style_class: `agent-notifier-dot agent-notifier-${state.css}`,
                y_align: Clutter.ActorAlign.CENTER,
            });
            if (session.state === 'working')
                this._workingDots.push(dot);

            const project = this._shorten(session.project || session.title || 'session', 12);
            chip.add_child(dot);
            chip.add_child(new St.Label({
                text: `${session.agent} · ${project}`,
                y_align: Clutter.ActorAlign.CENTER,
            }));
            this._panelBox.add_child(chip);
        }

        if (visible.length > MAX_PANEL_CHIPS) {
            this._panelBox.add_child(new St.Label({
                text: `+${visible.length - MAX_PANEL_CHIPS}`,
                style_class: 'agent-notifier-more',
                y_align: Clutter.ActorAlign.CENTER,
            }));
        }
    }

    _renderMenu(sessions) {
        this.menu.removeAll();
        const visibleSessions = sessions.filter(session =>
            Boolean(session.hidden) === this._showHidden);
        this.menu.addMenuItem(this._headerItem(visibleSessions, sessions));
        this.menu.addMenuItem(this._quickActions(sessions));

        if (!visibleSessions.length) {
            const empty = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
            empty.add_style_class_name('agent-notifier-empty');
            const historyHours = this._settings.get_uint('history-hours');
            empty.add_child(new St.Label({
                text: this._showHidden
                    ? 'No hidden sessions'
                    : `No sessions in the last ${historyHours} hours`,
            }));
            this.menu.addMenuItem(empty);
            return;
        }

        const section = new ScrollableSessionSection();
        this.menu.addMenuItem(section);
        for (const session of visibleSessions)
            section.addMenuItem(this._sessionCard(session));
    }

    _headerItem(sessions, allSessions) {
        const item = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        item.add_style_class_name('agent-notifier-menu-header');

        const titleBox = new St.BoxLayout({vertical: true, x_expand: true});
        titleBox.add_child(new St.Label({
            text: this._showHidden ? 'Hidden sessions' : 'Agent Notifier',
            style_class: 'agent-notifier-menu-title',
            x_align: Clutter.ActorAlign.START,
        }));
        const active = sessions.filter(session =>
            ['working', 'needs_attention'].includes(session.state)).length;
        const hidden = allSessions.filter(session => session.hidden).length;
        const historyHours = this._settings.get_uint('history-hours');
        titleBox.add_child(new St.Label({
            text: this._showHidden
                ? `${hidden} hidden · restore any session below`
                : `${active} active · ${sessions.length} recent · ${hidden} hidden · ${historyHours}h`,
            style_class: 'agent-notifier-muted',
            x_align: Clutter.ActorAlign.START,
        }));
        item.add_child(titleBox);

        const settings = new St.Button({
            style_class: 'agent-notifier-icon-button',
            can_focus: true,
            reactive: true,
            accessible_name: 'Open Agent Notifier settings',
        });
        settings.set_child(new St.Icon({
            icon_name: 'emblem-system-symbolic',
            style_class: 'popup-menu-icon',
        }));
        settings.connect('clicked', () => this._extension.openPreferences());
        item.add_child(settings);
        return item;
    }

    _quickActions(sessions) {
        const item = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        item.add_style_class_name('agent-notifier-quick-actions');
        const box = new St.BoxLayout({x_expand: true, style_class: 'agent-notifier-actions-box'});

        const enableAll = this._actionButton(
            'notifications-symbolic', 'Enable all', 'agent-notifier-action-enable');
        enableAll.connect('clicked', () => this._setAllNotifications(true, enableAll));
        box.add_child(enableAll);

        const disableAll = this._actionButton(
            'notifications-disabled-symbolic', 'Disable all', 'agent-notifier-action-disable');
        disableAll.connect('clicked', () => this._setAllNotifications(false, disableAll));
        box.add_child(disableAll);

        const hiddenCount = sessions.filter(session => session.hidden).length;
        const hidden = this._actionButton(
            this._showHidden ? 'go-previous-symbolic' : 'view-conceal-symbolic',
            this._showHidden ? 'Back to sessions' : `Hidden (${hiddenCount})`,
            'agent-notifier-action-hidden'
        );
        hidden.connect('clicked', () => {
            this._showHidden = !this._showHidden;
            this._renderMenu(sessions);
        });
        box.add_child(hidden);
        item.add_child(box);
        return item;
    }

    _actionButton(iconName, label, styleClass) {
        const button = new St.Button({
            style_class: `agent-notifier-action-button ${styleClass}`,
            can_focus: true,
            reactive: true,
            accessible_name: label,
        });
        const content = new St.BoxLayout({style_class: 'agent-notifier-button-content'});
        content.add_child(new St.Icon({icon_name: iconName, style_class: 'popup-menu-icon'}));
        content.add_child(new St.Label({text: label, y_align: Clutter.ActorAlign.CENTER}));
        button.set_child(content);
        return button;
    }

    _settingsItem() {
        const item = new PopupMenu.PopupMenuItem('Open Agent Notifier Settings');
        item.connect('activate', () => this._extension.openPreferences());
        return item;
    }

    _sessionCard(session) {
        const state = this._state(session.state);
        const card = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        card.add_style_class_name(
            `agent-notifier-card agent-notifier-card-${state.css}${session.hidden ? ' agent-notifier-card-hidden' : ''}`);

        const content = new St.BoxLayout({vertical: true, x_expand: true});
        const project = this._shorten(session.project || 'unknown', 18);
        const title = this._shorten(session.title || 'Untitled session', 30);
        const heading = new St.Label({
            text: `${session.agent} — ${project}/${title} — ${state.emoji} ${state.label}`,
            style_class: 'agent-notifier-card-title',
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        });
        heading.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        heading.clutter_text.single_line_mode = true;
        heading.set_width(MENU_TEXT_WIDTH);
        content.add_child(heading);

        const preview = new St.Label({
            text: this._shorten(session.preview || 'No response or question preview available.', 260),
            style_class: 'agent-notifier-card-preview',
            x_align: Clutter.ActorAlign.START,
        });
        preview.set_width(MENU_TEXT_WIDTH);
        preview.clutter_text.line_wrap = true;
        preview.clutter_text.line_wrap_mode = Pango.WrapMode.WORD_CHAR;
        preview.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
        content.add_child(preview);

        const footer = new St.BoxLayout({
            x_expand: true,
            style_class: 'agent-notifier-card-footer',
        });
        footer.add_child(new St.Label({
            text: `${state.emoji} ${state.label} · ${this._relative(session.updated_at)}`,
            style_class: `agent-notifier-card-status agent-notifier-${state.css}-text`,
            x_align: Clutter.ActorAlign.START,
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        }));

        const copyButton = this._actionButton(
            'edit-copy-symbolic', 'Copy resume', 'agent-notifier-card-copy');
        copyButton.add_style_class_name('agent-notifier-card-action');
        copyButton.connect('clicked', () => {
            St.Clipboard.get_default().set_text(
                St.ClipboardType.CLIPBOARD,
                this._resumeCommand(session)
            );
            this._setCopyButtonContent(copyButton, true);
        });
        footer.add_child(copyButton);

        const notifyButton = new St.Button({can_focus: true, reactive: true});
        this._setNotifyButtonContent(notifyButton, session.notifications_enabled, title);
        notifyButton.connect('clicked', () =>
            this._setSessionNotifications(session, notifyButton));
        footer.add_child(notifyButton);

        const hiddenLabel = session.hidden ? 'Restore' : 'Hide';
        const hideButton = this._actionButton(
            session.hidden ? 'view-reveal-symbolic' : 'view-conceal-symbolic',
            hiddenLabel,
            'agent-notifier-card-hide'
        );
        hideButton.add_style_class_name('agent-notifier-card-action');
        hideButton.connect('clicked', () =>
            this._setSessionHidden(session, !session.hidden, hideButton));
        footer.add_child(hideButton);

        content.add_child(footer);
        card.add_child(content);
        return card;
    }

    _setCopyButtonContent(button, copied) {
        const label = copied ? 'Copied' : 'Copy resume';
        button.accessible_name = copied ? 'Resume command copied' : 'Copy resume command';
        const content = new St.BoxLayout({style_class: 'agent-notifier-button-content'});
        content.add_child(new St.Icon({
            icon_name: copied ? 'object-select-symbolic' : 'edit-copy-symbolic',
            style_class: 'popup-menu-icon',
        }));
        content.add_child(new St.Label({text: label, y_align: Clutter.ActorAlign.CENTER}));
        button.set_child(content);
    }

    _resumeCommand(session) {
        const prefix = `${session.agent}:`;
        const rawId = session.id.startsWith(prefix)
            ? session.id.slice(prefix.length)
            : session.id;
        const id = this._shellArgument(rawId);
        if (session.agent === 'claude')
            return `claude --resume ${id}`;
        if (session.agent === 'opencode')
            return `opencode --session ${id}`;
        return `codex resume ${id}`;
    }

    _shellArgument(value) {
        const text = String(value);
        if (/^[A-Za-z0-9._:@+\/-]+$/.test(text))
            return text;
        return `'${text.replaceAll("'", "'\\''")}'`;
    }

    _setNotifyButtonContent(button, enabled, title) {
        button.set_style_class_name(
            `agent-notifier-card-action agent-notifier-notify-${enabled ? 'enabled' : 'disabled'}`);
        button.accessible_name = `${enabled ? 'Disable' : 'Enable'} notifications for ${title}`;
        const content = new St.BoxLayout({style_class: 'agent-notifier-button-content'});
        content.add_child(new St.Icon({
            icon_name: enabled ? 'notifications-symbolic' : 'notifications-disabled-symbolic',
            style_class: 'agent-notifier-large-action-icon',
        }));
        content.add_child(new St.Label({
            text: enabled ? 'Notify on' : 'Notify off',
            y_align: Clutter.ActorAlign.CENTER,
        }));
        button.set_child(content);
    }

    _setSessionNotifications(session, button) {
        const enabled = !session.notifications_enabled;
        button.reactive = false;
        this._proxy.SetSessionNotificationsRemote(session.id, enabled, (result, error) => {
            if (this._destroyed)
                return;
            button.reactive = true;
            if (error) {
                this._logError(error.message);
                return;
            }

            try {
                if (JSON.parse(result[0]).ok !== true)
                    return;
            } catch (parseError) {
                this._logError(`Invalid notification response: ${parseError.message}`);
                return;
            }

            session.notifications_enabled = enabled;
            this._setNotifyButtonContent(button, enabled, session.title);
        });
    }

    _setAllNotifications(enabled, button) {
        button.reactive = false;
        this._proxy.SetAllSessionNotificationsRemote(enabled, (result, error) => {
            if (this._destroyed)
                return;
            button.reactive = true;
            if (!this._responseOk(result, error, 'bulk notification update'))
                return;
            this._refresh();
        });
    }

    _setSessionHidden(session, hidden, button) {
        button.reactive = false;
        this._proxy.SetSessionHiddenRemote(session.id, hidden, (result, error) => {
            if (this._destroyed)
                return;
            button.reactive = true;
            if (!this._responseOk(result, error, 'session visibility update'))
                return;
            session.hidden = hidden;
            this._refresh();
        });
    }

    _responseOk(result, error, action) {
        if (error) {
            this._logError(`${action}: ${error.message}`);
            return false;
        }
        try {
            return JSON.parse(result[0]).ok === true;
        } catch (parseError) {
            this._logError(`${action}: ${parseError.message}`);
            return false;
        }
    }

    _state(value) {
        return STATE_META[value] || STATE_META.unknown;
    }

    _logError(message) {
        if (this._lastError === message)
            return;
        this._lastError = message;
        console.error(`Agent Notifier: ${message}`);
    }

    _shorten(value, maxLength) {
        const normalized = String(value || '').replace(/\s+/g, ' ').trim();
        return normalized.length > maxLength
            ? `${normalized.slice(0, maxLength - 1)}…`
            : normalized;
    }

    _timestamp(value) {
        const timestamp = GLib.DateTime.new_from_iso8601(value, null)?.to_unix();
        return timestamp ?? 0;
    }

    _relative(value) {
        const seconds = Math.max(0, Math.floor(Date.now() / 1000 - this._timestamp(value)));
        if (seconds < 60)
            return 'now';
        if (seconds < 3600)
            return `${Math.floor(seconds / 60)}m ago`;
        return `${Math.floor(seconds / 3600)}h ago`;
    }

    destroy() {
        this._destroyed = true;
        if (this._menuSignal)
            this.menu.disconnect(this._menuSignal);
        if (this._signal && this._proxy)
            this._proxy.disconnectSignal(this._signal);
        if (this._refreshTimer)
            GLib.source_remove(this._refreshTimer);
        if (this._blinkTimer)
            GLib.source_remove(this._blinkTimer);
        if (this._settingsChanged)
            this._settings.disconnect(this._settingsChanged);
        this._menuSignal = 0;
        this._signal = 0;
        this._refreshTimer = 0;
        this._blinkTimer = 0;
        this._settingsChanged = 0;
        this._proxy = null;
        this._settings = null;
        this._workingDots = [];
        this._lastError = null;
        this._extension = null;
        super.destroy();
    }
});

export default class AgentNotifierExtension extends Extension {
    enable() {
        this._indicator = new Indicator(this);
        Main.panel.addToStatusArea(this.uuid, this._indicator);
    }

    disable() {
        this._indicator?.destroy();
        this._indicator = null;
    }
}
