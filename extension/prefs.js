import Adw from 'gi://Adw';
import Gio from 'gi://Gio';
import Gtk from 'gi://Gtk';

import {ExtensionPreferences} from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

const IFACE = `<node><interface name="io.github.mmmohebi.AgentNotifier1"><method name="GetServiceStatus"><arg direction="out" type="s" name="json"/></method><method name="SetAllSessionNotifications"><arg direction="in" type="b" name="enabled"/><arg direction="out" type="s" name="json"/></method><method name="ConfigureDestination"><arg direction="in" type="s" name="name"/><arg direction="in" type="s" name="kind"/><arg direction="in" type="s" name="endpoint"/><arg direction="in" type="s" name="topic"/><arg direction="in" type="b" name="enabled"/><arg direction="in" type="s" name="token"/><arg direction="out" type="s" name="json"/></method><method name="RemoveDestination"><arg direction="in" type="s" name="name"/><arg direction="out" type="s" name="json"/></method><method name="TestDestination"><arg direction="in" type="s" name="name"/><arg direction="out" type="s" name="json"/></method></interface></node>`;
const Proxy = Gio.DBusProxy.makeProxyWrapper(IFACE);

export default class Preferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        this._closed = false;
        window.connect('close-request', () => {
            this._cleanup();
            return false;
        });
        window.set_default_size(680, 720);
        this._settings = this.getSettings();

        const page = new Adw.PreferencesPage({
            title: 'Agent Notifier',
            icon_name: 'system-run-symbolic',
        });
        window.add(page);

        this._buildHealthGroup(page);
        this._buildNotificationGroup(page);
        this._buildDisplayGroup(page);
        this._buildDestinationList(page);
        this._buildEditor(page);
        this._connectCompanion();
    }

    _cleanup() {
        this._closed = true;
        this._token.text = '';
        this._proxy = null;
        this._settings = null;
        this._healthRow = null;
        this._healthIcon = null;
        this._destinationGroup = null;
        this._destinationRows = [];
        this._notificationsEnabled = null;
        this._name = null;
        this._kind = null;
        this._endpoint = null;
        this._topic = null;
        this._token = null;
        this._enabled = null;
    }

    _buildHealthGroup(page) {
        const group = new Adw.PreferencesGroup({
            title: 'Companion',
            description: 'The local Rust service reads agent activity and sends notifications.',
        });
        this._healthRow = new Adw.ActionRow({
            title: 'Checking companion service…',
            subtitle: 'Connecting over the session D-Bus',
        });
        this._healthIcon = new Gtk.Image({icon_name: 'content-loading-symbolic'});
        this._healthRow.add_prefix(this._healthIcon);
        group.add(this._healthRow);
        page.add(group);
    }

    _buildDestinationList(page) {
        this._destinationGroup = new Adw.PreferencesGroup({
            title: 'Notification destinations',
            description: 'Enabled destinations receive attention, completion, and failure alerts independently.',
        });
        this._destinationRows = [];
        page.add(this._destinationGroup);
    }

    _buildNotificationGroup(page) {
        const group = new Adw.PreferencesGroup({
            title: 'Session notifications',
            description: 'This setting persists across reboots and applies to existing and future sessions.',
        });
        this._notificationsEnabled = new Adw.SwitchRow({
            title: 'Enable all session notifications',
            subtitle: 'Off by default; individual sessions can still be enabled from the popup',
            active: false,
        });
        this._notificationsEnabled.connect('notify::active', () => {
            if (!this._syncingNotifications)
                this._setAllNotifications(this._notificationsEnabled.active);
        });
        group.add(this._notificationsEnabled);
        page.add(group);
    }

    _buildDisplayGroup(page) {
        const group = new Adw.PreferencesGroup({
            title: 'Session display',
            description: 'Control the recent-session count and completed chips shown in the top bar.',
        });
        const history = new Adw.SpinRow({
            title: 'Recent session window',
            subtitle: 'Hours included in the popup and recent-session count',
            adjustment: new Gtk.Adjustment({
                lower: 1,
                upper: 24,
                step_increment: 1,
                page_increment: 1,
                value: this._settings.get_uint('history-hours'),
            }),
            digits: 0,
        });
        history.connect('notify::value', () =>
            this._settings.set_uint('history-hours', Math.round(history.value)));
        group.add(history);

        const completed = new Adw.SpinRow({
            title: 'Completed session visibility',
            subtitle: 'Minutes a finished session keeps its green top-bar chip',
            adjustment: new Gtk.Adjustment({
                lower: 1,
                upper: 120,
                step_increment: 1,
                page_increment: 5,
                value: this._settings.get_uint('topbar-completed-minutes'),
            }),
            digits: 0,
        });
        completed.connect('notify::value', () =>
            this._settings.set_uint(
                'topbar-completed-minutes', Math.round(completed.value)));
        group.add(completed);
        page.add(group);
    }

    _buildEditor(page) {
        const group = new Adw.PreferencesGroup({
            title: 'Add or edit a destination',
            description: 'Bearer tokens are stored in Secret Service. Leave the token blank when editing to keep the saved token.',
        });

        this._name = new Adw.EntryRow({title: 'Name'});
        this._name.text = 'default';
        this._kind = new Adw.ComboRow({
            title: 'Type',
            model: Gtk.StringList.new(['ntfy', 'webhook']),
            selected: 0,
        });
        this._endpoint = new Adw.EntryRow({title: 'ntfy server URL'});
        this._endpoint.text = 'https://ntfy.sh';
        this._topic = new Adw.EntryRow({title: 'ntfy topic'});
        this._token = new Adw.PasswordEntryRow({title: 'Bearer token (optional)'});
        this._enabled = new Adw.SwitchRow({title: 'Enabled', active: true});

        for (const row of [
            this._name,
            this._kind,
            this._endpoint,
            this._topic,
            this._token,
            this._enabled,
        ])
            group.add(row);

        this._kind.connect('notify::selected', () => this._syncKindFields());

        const actions = new Adw.ActionRow({
            title: 'Destination actions',
            subtitle: 'Save before sending a test notification.',
        });
        const testButton = new Gtk.Button({label: 'Send test', valign: Gtk.Align.CENTER});
        testButton.add_css_class('flat');
        testButton.connect('clicked', () => this._testDestination(this._name.text));
        const saveButton = new Gtk.Button({label: 'Save', valign: Gtk.Align.CENTER});
        saveButton.add_css_class('suggested-action');
        saveButton.connect('clicked', () => this._saveDestination());
        actions.add_suffix(testButton);
        actions.add_suffix(saveButton);
        group.add(actions);

        page.add(group);
        this._syncKindFields();
    }

    _connectCompanion() {
        try {
            this._proxy = new Proxy(
                Gio.DBus.session,
                'io.github.mmmohebi.AgentNotifier',
                '/io/github/mmmohebi/AgentNotifier',
                (_proxy, error) => {
                    if (this._closed)
                        return;
                    if (error) {
                        this._showDisconnected(error.message);
                        return;
                    }
                    this._loadStatus();
                }
            );
        } catch (error) {
            this._showDisconnected(error.message);
        }
    }

    _loadStatus(message = '') {
        if (this._closed || !this._proxy)
            return;

        this._proxy.GetServiceStatusRemote((result, error) => {
            if (this._closed)
                return;
            if (error) {
                this._showDisconnected(error.message);
                return;
            }

            try {
                const status = JSON.parse(result[0]);
                this._healthIcon.icon_name = 'emblem-ok-symbolic';
                this._healthRow.title = `Connected · companion ${status.version}`;
                this._healthRow.subtitle = message ||
                    `${status.sessions} sessions · ${status.destinations?.length || 0} destinations`;
                this._syncingNotifications = true;
                this._notificationsEnabled.active =
                    status.notifications_enabled_by_default === true;
                this._syncingNotifications = false;
                this._renderDestinations(status.destinations || []);
            } catch (parseError) {
                this._showDisconnected(`Invalid service response: ${parseError.message}`);
            }
        });
    }

    _showDisconnected(detail) {
        this._healthIcon.icon_name = 'dialog-error-symbolic';
        this._healthRow.title = 'Companion service is unavailable';
        this._healthRow.subtitle = `${detail}. Install or restart agent-notifier, then reopen Settings.`;
        this._renderDestinations([]);
    }

    _renderDestinations(destinations) {
        for (const row of this._destinationRows)
            this._destinationGroup.remove(row);
        this._destinationRows = [];

        if (!destinations.length) {
            const empty = new Adw.ActionRow({
                title: 'No destinations configured',
                subtitle: 'Use the form below to add ntfy or a generic webhook.',
            });
            empty.add_prefix(new Gtk.Image({icon_name: 'notifications-disabled-symbolic'}));
            this._destinationGroup.add(empty);
            this._destinationRows.push(empty);
            return;
        }

        for (const destination of destinations) {
            const subtitle = destination.kind === 'ntfy'
                ? `${destination.endpoint}/${destination.topic}${destination.has_token ? ' · authenticated' : ''}`
                : `${destination.endpoint}${destination.has_token ? ' · authenticated' : ''}`;
            const row = new Adw.ActionRow({
                title: destination.name,
                subtitle: `${destination.kind} · ${subtitle}`,
            });
            row.add_prefix(new Gtk.Image({
                icon_name: destination.kind === 'ntfy'
                    ? 'notifications-symbolic'
                    : 'network-transmit-symbolic',
            }));

            const enabled = new Gtk.Switch({
                active: destination.enabled,
                valign: Gtk.Align.CENTER,
                tooltip_text: 'Enable destination',
            });
            enabled.connect('notify::active', () =>
                this._toggleDestination(destination, enabled.active));
            row.add_suffix(enabled);

            const edit = this._iconButton('document-edit-symbolic', 'Edit destination');
            edit.connect('clicked', () => this._editDestination(destination));
            row.add_suffix(edit);

            const test = this._iconButton('mail-send-symbolic', 'Send test notification');
            test.connect('clicked', () => this._testDestination(destination.name));
            row.add_suffix(test);

            const remove = this._iconButton('user-trash-symbolic', 'Remove destination');
            remove.add_css_class('destructive-action');
            remove.connect('clicked', () => this._removeDestination(destination.name));
            row.add_suffix(remove);

            this._destinationGroup.add(row);
            this._destinationRows.push(row);
        }
    }

    _iconButton(iconName, tooltip) {
        const button = new Gtk.Button({
            icon_name: iconName,
            tooltip_text: tooltip,
            valign: Gtk.Align.CENTER,
        });
        button.add_css_class('flat');
        return button;
    }

    _syncKindFields() {
        const isNtfy = this._kind.selected === 0;
        this._endpoint.title = isNtfy ? 'ntfy server URL' : 'Webhook URL';
        this._topic.visible = isNtfy;
        if (isNtfy && !this._endpoint.text)
            this._endpoint.text = 'https://ntfy.sh';
    }

    _editDestination(destination) {
        this._name.text = destination.name;
        this._kind.selected = destination.kind === 'ntfy' ? 0 : 1;
        this._endpoint.text = destination.endpoint;
        this._topic.text = destination.topic || '';
        this._token.text = '';
        this._enabled.active = destination.enabled;
        this._healthRow.subtitle = `Editing “${destination.name}” · stored token will be kept if left blank`;
    }

    _saveDestination() {
        if (!this._proxy)
            return;

        const name = this._name.text.trim();
        const kind = this._kind.selected === 0 ? 'ntfy' : 'webhook';
        const endpoint = this._endpoint.text.trim();
        const topic = this._topic.text.trim();
        if (!name || !endpoint || (kind === 'ntfy' && !topic)) {
            this._healthRow.subtitle = 'Name, URL, and the ntfy topic are required.';
            return;
        }

        this._proxy.ConfigureDestinationRemote(
            name,
            kind,
            endpoint,
            topic,
            this._enabled.active,
            this._token.text,
            (result, error) => {
                if (this._closed)
                    return;
                const response = this._response(result, error);
                if (!response.ok) {
                    this._healthRow.subtitle = `Could not save: ${response.error}`;
                    return;
                }
                this._token.text = '';
                this._loadStatus(`Saved destination “${name}”`);
            }
        );
    }

    _toggleDestination(destination, enabled) {
        this._proxy?.ConfigureDestinationRemote(
            destination.name,
            destination.kind,
            destination.endpoint,
            destination.topic || '',
            enabled,
            '',
            (result, error) => {
                if (this._closed)
                    return;
                const response = this._response(result, error);
                this._loadStatus(response.ok
                    ? `${enabled ? 'Enabled' : 'Disabled'} “${destination.name}”`
                    : `Could not update: ${response.error}`);
            }
        );
    }

    _setAllNotifications(enabled) {
        if (!this._proxy)
            return;
        this._proxy.SetAllSessionNotificationsRemote(enabled, (result, error) => {
            if (this._closed)
                return;
            const response = this._response(result, error);
            if (response.ok) {
                this._healthRow.subtitle = enabled
                    ? 'All current and future session notifications enabled'
                    : 'All current and future session notifications disabled';
                return;
            }
            this._healthRow.subtitle = `Could not update notifications: ${response.error}`;
            this._loadStatus();
        });
    }

    _testDestination(name) {
        if (!this._proxy || !name.trim()) {
            this._healthRow.subtitle = 'Choose or save a destination before testing.';
            return;
        }

        this._healthRow.subtitle = `Sending a test to “${name}”…`;
        this._proxy.TestDestinationRemote(name.trim(), (result, error) => {
            if (this._closed)
                return;
            const response = this._response(result, error);
            this._healthRow.subtitle = response.ok
                ? `Test delivered to “${name}”`
                : `Test failed: ${response.error}`;
        });
    }

    _removeDestination(name) {
        this._proxy?.RemoveDestinationRemote(name, (result, error) => {
            if (this._closed)
                return;
            const response = this._response(result, error);
            this._loadStatus(response.ok
                ? `Removed destination “${name}”`
                : `Could not remove: ${response.error}`);
        });
    }

    _response(result, error) {
        if (error)
            return {ok: false, error: error.message};
        try {
            return JSON.parse(result[0]);
        } catch (parseError) {
            return {ok: false, error: `Invalid service response: ${parseError.message}`};
        }
    }
}
