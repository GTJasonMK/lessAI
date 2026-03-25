import { memo } from "react";
import { LoaderCircle } from "lucide-react";

export const BootScreen = memo(function BootScreen() {
  return (
    <div className="boot-screen">
      <div className="boot-card">
        <LoaderCircle className="spin" />
        <div>
          <p>LessAI</p>
          <strong>正在装载单屏工作台</strong>
        </div>
      </div>
    </div>
  );
});

