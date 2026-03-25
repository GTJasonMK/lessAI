import { memo } from "react";
import type { ResizeDirection } from "../hooks/useWindowControls";

interface WindowResizeLayerProps {
  onResize: (direction: ResizeDirection) => Promise<void>;
}

export const WindowResizeLayer = memo(function WindowResizeLayer({
  onResize
}: WindowResizeLayerProps) {
  return (
    <div className="window-resize-layer" aria-hidden="true">
      <button
        type="button"
        className="resize-handle is-n"
        tabIndex={-1}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          void onResize("North");
        }}
      />
      <button
        type="button"
        className="resize-handle is-e"
        tabIndex={-1}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          void onResize("East");
        }}
      />
      <button
        type="button"
        className="resize-handle is-s"
        tabIndex={-1}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          void onResize("South");
        }}
      />
      <button
        type="button"
        className="resize-handle is-w"
        tabIndex={-1}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          void onResize("West");
        }}
      />

      <button
        type="button"
        className="resize-handle is-nw"
        tabIndex={-1}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          void onResize("NorthWest");
        }}
      />
      <button
        type="button"
        className="resize-handle is-ne"
        tabIndex={-1}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          void onResize("NorthEast");
        }}
      />
      <button
        type="button"
        className="resize-handle is-se"
        tabIndex={-1}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          void onResize("SouthEast");
        }}
      />
      <button
        type="button"
        className="resize-handle is-sw"
        tabIndex={-1}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          void onResize("SouthWest");
        }}
      />
    </div>
  );
});
