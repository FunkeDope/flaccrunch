import { useState, useEffect } from "react";
import { Header } from "../layout/Header";
import * as api from "../../lib/tauri";

export function LogViewer() {
  const [logContent, setLogContent] = useState("");
  const [filter, setFilter] = useState("");

  useEffect(() => {
    api.getRunLog().then(setLogContent).catch(() => {});
  }, []);

  const filteredContent = filter
    ? logContent
        .split("\n")
        .filter((line) => line.toLowerCase().includes(filter.toLowerCase()))
        .join("\n")
    : logContent;

  return (
    <div>
      <Header title="Logs" />

      <div style={{ marginBottom: 12 }}>
        <input
          type="text"
          placeholder="Filter logs..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          style={{
            width: "100%",
            padding: "8px 12px",
            background: "var(--bg-secondary)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            color: "var(--text-primary)",
            fontSize: 14,
          }}
        />
      </div>

      <div className="log-viewer">
        {filteredContent || "No log data available. Start processing to generate logs."}
      </div>
    </div>
  );
}
