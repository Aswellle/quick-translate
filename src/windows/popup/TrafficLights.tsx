// src/windows/popup/TrafficLights.tsx
// macOS 风格红绿灯控件（浮窗左上角）
//
// 语义针对无边框置顶浮窗做了适配：popup 是 decorations(false) + skip_taskbar(true)，
// 没有任务栏入口，真正 minimize() 会让窗口消失且无处可点回。因此：
//   🔴 关闭    🟡 折叠成紧凑条（再点展开）    🟢 标准宽 / 阅读宽 切换

interface TrafficLightsProps {
  onClose: () => void;
  onToggleCollapse: () => void;
  onToggleWide: () => void;
  collapsed: boolean;
  wide: boolean;
}

export function TrafficLights({
  onClose,
  onToggleCollapse,
  onToggleWide,
  collapsed,
  wide,
}: TrafficLightsProps) {
  return (
    // group：hover 任意一个按钮时统一显出所有字形，与 macOS 行为一致
    // data-no-drag：阻止 PopupWindow 的 mousedown 拖拽逻辑吞掉点击
    <div className="traffic-lights group" data-no-drag>
      <TrafficLight
        variant="close"
        title="关闭 (Esc / 空格)"
        onClick={onClose}
        glyph={
          <path
            d="M3.4 3.4l5.2 5.2M8.6 3.4L3.4 8.6"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
          />
        }
      />
      <TrafficLight
        variant="collapse"
        title={collapsed ? "展开" : "折叠"}
        onClick={onToggleCollapse}
        glyph={
          collapsed ? (
            <path
              d="M3.2 6h5.6M6 3.2v5.6"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
            />
          ) : (
            <path
              d="M3.2 6h5.6"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
            />
          )
        }
      />
      <TrafficLight
        variant="wide"
        title={wide ? "标准宽度" : "阅读宽度"}
        onClick={onToggleWide}
        disabled={collapsed}
        glyph={
          wide ? (
            <path
              d="M7.6 4.4h-3v3M4.4 7.6l3.2-3.2"
              stroke="currentColor"
              strokeWidth="1.4"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          ) : (
            <path
              d="M4.4 7.6V4.4h3.2M4.4 4.4l3.2 3.2"
              stroke="currentColor"
              strokeWidth="1.4"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          )
        }
      />
    </div>
  );
}

function TrafficLight({
  variant,
  title,
  onClick,
  glyph,
  disabled = false,
}: {
  variant: "close" | "collapse" | "wide";
  title: string;
  onClick: () => void;
  glyph: React.ReactNode;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={title}
      title={title}
      onClick={onClick}
      disabled={disabled}
      className={`traffic-light traffic-light--${variant}`}
    >
      <svg viewBox="0 0 12 12" fill="none" aria-hidden="true">
        {glyph}
      </svg>
    </button>
  );
}
