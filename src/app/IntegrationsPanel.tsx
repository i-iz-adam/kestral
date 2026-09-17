import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Connection } from "../types";

export interface ConnectionTypeDef {
  id: string;
  name: string;
  description: string;
  color: string;
  badgeBg: string;
  icon: React.ReactNode;
  fields: {
    key: string;
    label: string;
    type?: "text" | "password";
    placeholder?: string;
    hint?: string;
    required?: boolean;
  }[];
}

const CONNECTION_TYPES: ConnectionTypeDef[] = [
  {
    id: "github",
    name: "GitHub",
    description: "Connect GitHub accounts for managing repositories, issues, and pull requests.",
    color: "#ffffff",
    badgeBg: "rgba(255, 255, 255, 0.1)",
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
        <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" />
      </svg>
    ),
    fields: [
      { key: "token", label: "Personal Access Token", type: "password", placeholder: "ghp_...", required: true, hint: "GitHub PAT with repo, issue & PR permissions." },
      { key: "owner", label: "Default Owner / Org", type: "text", placeholder: "acme-corp", hint: "Optional default owner for repo operations." },
      { key: "repo", label: "Default Repository", type: "text", placeholder: "main-app", hint: "Optional default repository name." },
    ],
  },
  {
    id: "discord",
    name: "Discord (Bot)",
    description: "Bot connection for managing servers, sending messages, creating roles & channels.",
    color: "#5865F2",
    badgeBg: "rgba(88, 101, 242, 0.15)",
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
        <path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028c.462-.63.874-1.295 1.226-1.994.021-.041.001-.09-.041-.106a13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128c.126-.093.252-.19.372-.287a.075.075 0 0 1 .078-.01c3.927 1.793 8.18 1.793 12.061 0a.074.074 0 0 1 .079.009c.12.098.245.195.372.288a.077.077 0 0 1-.006.128c-.598.35-1.22.647-1.873.892-.041.016-.062.064-.041.106.357.698.769 1.364 1.225 1.994a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.028zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z" />
      </svg>
    ),
    fields: [
      { key: "token", label: "Bot Token", type: "password", placeholder: "MT... or Bot token", required: true, hint: "Bot Token from Discord Developer Portal." },
      { key: "guild_id", label: "Server / Guild ID", type: "text", placeholder: "123456789012345678", hint: "Target Discord Server ID for role & channel actions." },
      { key: "channel_id", label: "Default Channel ID", type: "text", placeholder: "123456789012345678", hint: "Optional default text channel ID for posting messages." },
    ],
  },
  {
    id: "slack",
    name: "Slack (Bot)",
    description: "Slack app integration for channels, messaging, and operational alerts.",
    color: "#4A154B",
    badgeBg: "rgba(74, 21, 75, 0.2)",
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
        <path d="M5.042 15.165a2.528 2.528 0 0 1-2.52 2.523A2.528 2.528 0 0 1 0 15.165a2.527 2.527 0 0 1 2.522-2.52h2.52v2.52zm1.271 0a2.527 2.527 0 0 1 2.521-2.52 2.527 2.527 0 0 1 2.521 2.52v6.313A2.528 2.528 0 0 1 8.834 24a2.528 2.528 0 0 1-2.521-2.522v-6.313zM8.834 5.042a2.528 2.528 0 0 1-2.521-2.52A2.528 2.528 0 0 1 8.834 0a2.528 2.528 0 0 1 2.521 2.522v2.52H8.834zm0 1.271a2.528 2.528 0 0 1 2.521 2.521 2.528 2.528 0 0 1-2.521 2.521H2.522A2.528 2.528 0 0 1 0 8.834a2.528 2.528 0 0 1 2.522-2.521h6.312zm10.122 2.521a2.528 2.528 0 0 1 2.522-2.521A2.528 2.528 0 0 1 24 8.834a2.528 2.528 0 0 1-2.522 2.521h-2.522V8.834zm-1.268 0a2.528 2.528 0 0 1-2.523 2.521 2.527 2.527 0 0 1-2.52-2.521V2.522A2.527 2.527 0 0 1 15.165 0a2.528 2.528 0 0 1 2.523 2.522v6.312zm0 10.123a2.528 2.528 0 0 1 2.523 2.52 2.528 2.528 0 0 1-2.523 2.523h-2.52v-2.522zm0-1.268a2.527 2.527 0 0 1-2.523-2.521 2.527 2.527 0 0 1 2.523-2.521h6.313A2.528 2.528 0 0 1 24 15.165a2.528 2.528 0 0 1-2.522 2.523h-6.313z" />
      </svg>
    ),
    fields: [
      { key: "token", label: "Bot User OAuth Token", type: "password", placeholder: "xoxb-...", required: true, hint: "OAuth token starting with xoxb-." },
      { key: "channel_id", label: "Default Channel ID", type: "text", placeholder: "C0123456789", hint: "Target channel ID for messages." },
    ],
  },
  {
    id: "telegram",
    name: "Telegram",
    description: "Telegram bot for instant notifications and chat commands.",
    color: "#26A5E4",
    badgeBg: "rgba(38, 165, 228, 0.15)",
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
        <path d="M11.944 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0a12 12 0 0 0-.056 0zm4.962 7.224c.1-.002.321.023.465.14a.506.506 0 0 1 .171.325c.016.093.036.306.02.472-.18 1.898-.962 6.502-1.36 8.627-.168.9-.499 1.201-.82 1.23-.696.065-1.225-.46-1.901-.903-1.056-.693-1.653-1.124-2.678-1.8-1.185-.78-.417-1.21.258-1.91.177-.184 3.247-2.977 3.307-3.23.007-.032.014-.15-.056-.212s-.174-.041-.249-.024c-.106.024-1.793 1.14-5.061 3.345-.48.33-.913.49-1.302.48-.428-.008-1.252-.241-1.865-.44-.752-.245-1.349-.374-1.297-.789.027-.216.325-.437.893-.663 3.498-1.524 5.831-2.529 6.998-3.014 3.332-1.386 4.025-1.627 4.476-1.635z" />
      </svg>
    ),
    fields: [
      { key: "token", label: "Bot API Token", type: "password", placeholder: "123456789:ABC...", required: true, hint: "Bot Token from @BotFather on Telegram." },
      { key: "chat_id", label: "Default Chat ID", type: "text", placeholder: "-100123456789", hint: "Target user or channel chat ID." },
    ],
  },
  {
    id: "notion",
    name: "Notion",
    description: "Connect Notion workspace databases, pages, and notes.",
    color: "#ffffff",
    badgeBg: "rgba(255, 255, 255, 0.1)",
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
        <path d="M4.459 4.208c.746.606 1.026.56 2.428.466l13.215-.793c.28 0 .047-.28-.046-.326L17.86 1.968c-.42-.326-.981-.7-2.055-.606L3.339 2.574c-.467.047-.56.28-.374.467zm.793 4.576v12.934c0 .793.374 1.073 1.12 1.026l14.288-.84c.747-.046.934-.513.934-1.213V7.757c0-.7-.28-.98-.934-.933l-14.288.84c-.746.047-1.12.373-1.12 1.12zm13.122 1.213c.046.28 0 .56-.28.56l-.373.047c-.513.047-.793.28-.793.746v8.406c-.467.42-.98.653-1.4.653-.654 0-.934-.326-1.447-.98l-4.576-7.238v6.864c0 .653.326.887.933.933l.42.047c.28 0 .327.28.28.56l-2.615.14c-.047.28-.28.28-.42.28-.654-.047-.934-.327-.934-.98V9.717c0-.653.28-.887.887-.933l2.848-.187c.607-.047 1.027.047 1.494.747l4.67 7.237V9.764c0-.654-.327-.887-.934-.934l-.42-.047c-.28 0-.326-.28-.28-.56z" />
      </svg>
    ),
    fields: [
      { key: "token", label: "Internal Integration Token", type: "password", placeholder: "secret_...", required: true, hint: "Integration Token from notion.so/my-integrations." },
      { key: "database_id", label: "Default Database ID", type: "text", placeholder: "32-character hex ID", hint: "Target database ID in Notion." },
    ],
  },
  {
    id: "linear",
    name: "Linear",
    description: "Linear issue tracker integration for syncing tasks and sprint items.",
    color: "#5E6AD2",
    badgeBg: "rgba(94, 106, 210, 0.15)",
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
        <path d="M2.5 12a9.5 9.5 0 1 1 19 0 9.5 9.5 0 0 1-19 0zm9.5-8a8 8 0 1 0 0 16 8 8 0 0 0 0-16zm-3.5 8a3.5 3.5 0 1 1 7 0 3.5 3.5 0 0 1-7 0z" />
      </svg>
    ),
    fields: [
      { key: "token", label: "API Key", type: "password", placeholder: "lin_api_...", required: true, hint: "Personal API key from Linear Settings > API." },
      { key: "team_key", label: "Default Team Key", type: "text", placeholder: "ENG", hint: "Target team short identifier (e.g. ENG, DEV)." },
    ],
  },
  {
    id: "webhook",
    name: "Custom Webhook",
    description: "Trigger HTTP webhooks or custom REST APIs with authentication headers.",
    color: "#F59E0B",
    badgeBg: "rgba(245, 158, 11, 0.15)",
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
        <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
      </svg>
    ),
    fields: [
      { key: "endpoint_url", label: "Endpoint URL", type: "text", placeholder: "https://api.example.com/webhook", required: true, hint: "Full HTTP/HTTPS webhook target URL." },
      { key: "token", label: "Bearer Token / Secret Key", type: "password", placeholder: "Bearer token or API key", hint: "Optional Authorization header token." },
    ],
  },
  {
    id: "postgres",
    name: "PostgreSQL / DB",
    description: "Direct database connection for schema inspection and queries.",
    color: "#336791",
    badgeBg: "rgba(51, 103, 145, 0.2)",
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
        <path d="M12 2C6.48 2 2 4.02 2 6.5s4.48 4.5 10 4.5 10-2.02 10-4.5S17.52 2 12 2zm0 6.5c-4.42 0-8-1.57-8-2.5s3.58-2.5 8-2.5 8 1.57 8 2.5-3.58 2.5-8 2.5zM2 9v4.5C2 16 6.48 18 12 18s10-2 10-4.5V9c-2.12 1.76-5.83 2.5-10 2.5S4.12 10.76 2 9zm0 6.5V20c0 2.5 4.48 4.5 10 4.5s10-2 10-4.5v-4.5c-2.12 1.76-5.83 2.5-10 2.5S4.12 17.26 2 15.5z" />
      </svg>
    ),
    fields: [
      { key: "connection_string", label: "Connection String", type: "password", placeholder: "postgresql://user:pass@localhost:5432/dbname", required: true, hint: "Postgres connection URI string." },
    ],
  },
];

export default function IntegrationsPanel() {
  const [connections, setConnections] = useState<Connection[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [activeModalTypeDef, setActiveModalTypeDef] = useState<ConnectionTypeDef | null>(null);
  const [editingConn, setEditingConn] = useState<Connection | null>(null);
  const [formData, setFormData] = useState<{ name: string; config: Record<string, string> }>({
    name: "",
    config: {},
  });

  const [showPassword, setShowPassword] = useState<Record<string, boolean>>({});
  const [busy, setBusy] = useState(false);
  const [testingStatus, setTestingStatus] = useState<{ loading: boolean; message?: string; isError?: boolean } | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);

  const loadConnections = async () => {
    try {
      const list = await invoke<Connection[]>("get_connections");
      setConnections(list);
    } catch (e) {
      console.error("Failed to load connections:", e);
    }
  };

  useEffect(() => {
    loadConnections();
  }, []);

  const openModalForType = (typeDef: ConnectionTypeDef, connToEdit?: Connection) => {
    setActiveModalTypeDef(typeDef);
    setTestingStatus(null);
    setShowPassword({});
    if (connToEdit) {
      setEditingConn(connToEdit);
      setFormData({
        name: connToEdit.name,
        config: { ...connToEdit.config },
      });
    } else {
      setEditingConn(null);
      // Auto-generate a sensible default connection name, e.g. "The Magician" or "Main Discord Bot"
      const typeConns = connections.filter((c) => c.type === typeDef.id);
      const countLabel = typeConns.length > 0 ? ` ${typeConns.length + 1}` : "";
      const defaultName = typeDef.id === "discord" && typeConns.length === 0 ? "The Magician" : `${typeDef.name}${countLabel}`;
      setFormData({
        name: defaultName,
        config: {},
      });
    }
  };

  const closeModal = () => {
    setActiveModalTypeDef(null);
    setEditingConn(null);
    setTestingStatus(null);
  };

  const handleFieldChange = (key: string, value: string) => {
    setFormData((prev) => ({
      ...prev,
      config: { ...prev.config, [key]: value },
    }));
  };

  const testCurrentForm = async () => {
    if (!activeModalTypeDef) return;
    setTestingStatus({ loading: true });
    const tempConn: Connection = {
      id: editingConn?.id || "temp",
      type: activeModalTypeDef.id,
      name: formData.name.trim() || activeModalTypeDef.name,
      created_at: editingConn?.created_at || Date.now(),
      updated_at: Date.now(),
      status: "untested",
      config: formData.config,
    };

    try {
      const tested = await invoke<Connection>("test_connection", { connection: tempConn });
      if (tested.status === "connected") {
        setTestingStatus({
          loading: false,
          isError: false,
          message: `Verified successfully! ${tested.account_name ? `(${tested.account_name})` : ""}`,
        });
      } else {
        setTestingStatus({
          loading: false,
          isError: true,
          message: `Test failed: ${tested.account_name || "Unable to authenticate"}`,
        });
      }
    } catch (err) {
      setTestingStatus({
        loading: false,
        isError: true,
        message: String(err),
      });
    }
  };

  const saveConnectionForm = async () => {
    if (!activeModalTypeDef || !formData.name.trim()) return;
    setBusy(true);
    setTestingStatus({ loading: true });

    const connToSave: Connection = {
      id: editingConn?.id || `conn_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`,
      type: activeModalTypeDef.id,
      name: formData.name.trim(),
      created_at: editingConn?.created_at || Date.now(),
      updated_at: Date.now(),
      status: "untested",
      account_name: editingConn?.account_name,
      config: formData.config,
    };

    try {
      const saved = await invoke<Connection>("save_connection", { connection: connToSave });
      await loadConnections();
      closeModal();
      setActionMessage(`Saved connection "${saved.name}" (${saved.status === "connected" ? "Connected" : "Saved"})`);
      setTimeout(() => setActionMessage(null), 3500);
    } catch (e) {
      setTestingStatus({
        loading: false,
        isError: true,
        message: String(e),
      });
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async (conn: Connection) => {
    if (!window.confirm(`Are you sure you want to remove connection "${conn.name}"?`)) return;
    try {
      await invoke("delete_connection", { id: conn.id });
      await loadConnections();
      setActionMessage(`Removed connection "${conn.name}"`);
      setTimeout(() => setActionMessage(null), 3000);
    } catch (e) {
      alert(`Failed to delete: ${String(e)}`);
    }
  };

  const handleTestConnection = async (conn: Connection) => {
    try {
      const tested = await invoke<Connection>("save_connection", { connection: conn });
      await loadConnections();
      setActionMessage(
        tested.status === "connected"
          ? `✓ Connection "${conn.name}" verified! ${tested.account_name || ""}`
          : `❌ Connection "${conn.name}" error: ${tested.account_name || "failed"}`
      );
      setTimeout(() => setActionMessage(null), 4000);
    } catch (e) {
      alert(`Test error: ${String(e)}`);
    }
  };

  const filteredTypes = CONNECTION_TYPES.filter(
    (t) =>
      t.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      t.description.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <div className="integrations-settings-panel">
      {/* Banner / Header */}
      <div className="integrations-header">
        <div className="integrations-title-row">
          <h2>Integrations & Connections</h2>
          {actionMessage && <div className="integrations-toast">{actionMessage}</div>}
        </div>
        <p className="integrations-subtitle">
          Connect external services, bots, and platforms. You can configure multiple connections of the same type and give them custom names (e.g. <code>"The Magician"</code>) to use in prompts.
        </p>
      </div>

      {/* Search / Filter */}
      <div className="integrations-search-bar">
        <input
          type="text"
          placeholder="Filter connection types (GitHub, Discord, Slack, Telegram...)..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
        />
      </div>

      {/* Grid of Connection Type Buttons */}
      <div className="connection-types-grid">
        {filteredTypes.map((typeDef) => {
          const typeConns = connections.filter((c) => c.type === typeDef.id);
          const activeCount = typeConns.length;
          return (
            <button
              key={typeDef.id}
              className="connection-type-button"
              onClick={() => openModalForType(typeDef)}
            >
              <div className="type-icon-wrapper" style={{ color: typeDef.color }}>
                {typeDef.icon}
              </div>
              <div className="type-info">
                <span className="type-name">{typeDef.name}</span>
                <span className="type-desc">{typeDef.description}</span>
              </div>
              <div className="type-badge-col">
                {activeCount > 0 ? (
                  <span className="count-badge active">
                    <span className="dot online" /> {activeCount} connected
                  </span>
                ) : (
                  <span className="count-badge add">+ Add</span>
                )}
              </div>
            </button>
          );
        })}
      </div>

      {/* Active Connections List */}
      <div className="active-connections-section">
        <div className="section-header-row">
          <h3>Active Configured Connections ({connections.length})</h3>
        </div>

        {connections.length === 0 ? (
          <div className="empty-connections-card">
            <span className="empty-icon">🌐</span>
            <p>No connections configured yet.</p>
            <p className="hint">Click any connection type button above (such as <strong>Discord</strong> or <strong>GitHub</strong>) to add a named connection!</p>
          </div>
        ) : (
          <div className="connections-list-grid">
            {connections.map((conn) => {
              const typeDef = CONNECTION_TYPES.find((t) => t.id === conn.type);
              return (
                <div key={conn.id} className={`connection-card ${conn.status}`}>
                  <div className="conn-card-header">
                    <div className="conn-type-badge" style={{ color: typeDef?.color || "#fff" }}>
                      {typeDef?.icon || "🔌"}
                      <span>{typeDef?.name || conn.type}</span>
                    </div>

                    <div className="conn-status-indicator">
                      {conn.status === "connected" ? (
                        <span className="status-pill connected">
                          <span className="dot online" /> {conn.account_name || "Connected"}
                        </span>
                      ) : conn.status === "error" ? (
                        <span className="status-pill error">
                          <span className="dot error" /> {conn.account_name || "Error"}
                        </span>
                      ) : (
                        <span className="status-pill untested">
                          <span className="dot idle" /> Untested
                        </span>
                      )}
                    </div>
                  </div>

                  <div className="conn-card-body">
                    <h4 className="conn-name">{conn.name}</h4>
                    <p className="conn-prompt-hint">
                      Prompt: <code>"Using {conn.name}, create..."</code>
                    </p>
                  </div>

                  <div className="conn-card-actions">
                    <button className="small button-test" onClick={() => handleTestConnection(conn)}>
                      Test
                    </button>
                    <button
                      className="small button-edit"
                      onClick={() => typeDef && openModalForType(typeDef, conn)}
                    >
                      Edit
                    </button>
                    <button className="small button-delete" onClick={() => handleDelete(conn)}>
                      Remove
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Modal for Managing Connections of a Specific Type */}
      {activeModalTypeDef && (
        <div className="connection-modal-backdrop" onClick={closeModal}>
          <div className="connection-modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <div className="modal-title-group" style={{ color: activeModalTypeDef.color }}>
                {activeModalTypeDef.icon}
                <h3>
                  {editingConn ? `Edit "${editingConn.name}"` : `Add ${activeModalTypeDef.name} Connection`}
                </h3>
              </div>
              <button className="modal-close-btn" onClick={closeModal}>
                ✕
              </button>
            </div>

            {/* List of existing connections for this type if adding fresh */}
            {!editingConn && (
              <div className="type-existing-connections">
                {connections.filter((c) => c.type === activeModalTypeDef.id).length > 0 && (
                  <div className="existing-list-box">
                    <span className="existing-title">Configured {activeModalTypeDef.name} Connections:</span>
                    <div className="existing-tags">
                      {connections
                        .filter((c) => c.type === activeModalTypeDef.id)
                        .map((c) => (
                          <button
                            key={c.id}
                            className="existing-conn-tag"
                            onClick={() => openModalForType(activeModalTypeDef, c)}
                          >
                            <span className={`status-dot ${c.status}`} />
                            <span className="tag-name">{c.name}</span>
                            <span className="tag-account">{c.account_name || ""}</span>
                          </button>
                        ))}
                    </div>
                  </div>
                )}
              </div>
            )}

            <div className="modal-form-body">
              <div className="field-group">
                <label>
                  Connection Name <span className="req">*</span>
                </label>
                <input
                  type="text"
                  placeholder="e.g. The Magician, Work GitHub, Support Bot"
                  value={formData.name}
                  onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                />
                <p className="hint small">
                  Give this connection a unique name so you can reference it directly in prompts (e.g., <em>"Using {formData.name || 'The Magician'}, create a role for moderators"</em>).
                </p>
              </div>

              {activeModalTypeDef.fields.map((field) => (
                <div key={field.key} className="field-group">
                  <label>
                    {field.label} {field.required && <span className="req">*</span>}
                  </label>
                  <div className="input-with-toggle">
                    <input
                      type={field.type === "password" && !showPassword[field.key] ? "password" : "text"}
                      placeholder={field.placeholder}
                      value={formData.config[field.key] || ""}
                      onChange={(e) => handleFieldChange(field.key, e.target.value)}
                    />
                    {field.type === "password" && (
                      <button
                        type="button"
                        className="toggle-password-btn"
                        onClick={() =>
                          setShowPassword({
                            ...showPassword,
                            [field.key]: !showPassword[field.key],
                          })
                        }
                      >
                        {showPassword[field.key] ? "Hide" : "Show"}
                      </button>
                    )}
                  </div>
                  {field.hint && <p className="hint small">{field.hint}</p>}
                </div>
              ))}

              {testingStatus && (
                <div className={`modal-test-status ${testingStatus.isError ? "error" : "success"}`}>
                  {testingStatus.loading ? (
                    <span className="testing-spinner">Testing connection credentials...</span>
                  ) : (
                    <span>{testingStatus.message}</span>
                  )}
                </div>
              )}
            </div>

            <div className="modal-actions">
              <button
                type="button"
                className="secondary"
                onClick={testCurrentForm}
                disabled={testingStatus?.loading || busy}
              >
                Test Credentials
              </button>
              <div className="actions-right">
                <button type="button" onClick={closeModal} disabled={busy}>
                  Cancel
                </button>
                <button
                  type="button"
                  className="primary"
                  onClick={saveConnectionForm}
                  disabled={!formData.name.trim() || busy}
                >
                  {busy ? "Saving..." : editingConn ? "Update Connection" : "Save Connection"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
