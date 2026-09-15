import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { initTheme } from "./lib/theme";
import { platform } from "./lib/platform";
import "./index.css";

// Restore the stored theme before the first paint to avoid flashing the
// wrong one (design.md §4.4); app_settings reconciles once loaded.
initTheme();
document.documentElement.dataset.platform = platform;

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // The database is the single source of truth and lives on the same
      // machine, so refetching is cheap. Freshness is driven by Tauri events
      // (see docs/architecture.md, decision D4).
      staleTime: 0,
      refetchOnWindowFocus: true,
      retry: 1,
    },
  },
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
);
