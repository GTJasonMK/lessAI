import { memo } from "react";
import { MoonStar, SunMedium } from "lucide-react";

interface ThemeToggleProps {
  themeMode: "light" | "dark";
  onToggle: () => void;
}

export const ThemeToggle = memo(function ThemeToggle({
  themeMode,
  onToggle
}: ThemeToggleProps) {
  const nextModeLabel = themeMode === "dark" ? "亮色" : "暗色";
  const title = `切换到${nextModeLabel}主题`;
  const Icon = themeMode === "dark" ? SunMedium : MoonStar;

  return (
    <button
      type="button"
      className="theme-toggle"
      data-window-drag-exclude="true"
      aria-label={title}
      aria-pressed={themeMode === "dark"}
      title={title}
      onClick={onToggle}
    >
      <Icon />
    </button>
  );
});
