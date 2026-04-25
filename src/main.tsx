import React from "react";
import ReactDOM from "react-dom/client";

async function bootstrap() {
  const rootElement = document.getElementById("root");
  if (!rootElement) {
    throw new Error("Root container not found");
  }

  const root = ReactDOM.createRoot(rootElement);
  await import("./styles.css");
  const [{ default: App }, { attachRuntimeConsole }] = await Promise.all([
    import("./App"),
    import("./lib/runtimeLog")
  ]);

  try {
    await attachRuntimeConsole();
  } catch {
    // 非 Tauri 环境下允许静默失败，避免阻塞前端开发。
  }

  root.render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  );
}

void bootstrap();
