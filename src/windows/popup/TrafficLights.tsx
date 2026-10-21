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
  /** 浮窗已转为可交互态：关闭 tooltip 才提 Esc/空格（被动态收不到键盘） */
  activated: boolean;
}

export function TrafficLights({
  onClose,
  onToggleCollapse,
  onToggleWide,
  collapsed,
  wide,
  activated,
}: TrafficLightsProps) {
  return (
    // group：hover 任意一个按钮时统一显出所有字形，与 macOS 行为一致
    // data-no-drag：阻止 PopupWindow 的 mousedown 拖拽逻辑吞掉点击
    <div className="traffic-lights group" data-no-drag>
      <TrafficLight
        variant="close"
        title={activated ? "关闭 (Esc / 空格)" : "关闭"}
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
        ariaExpanded={!collapsed}
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
        ariaPressed={wide}
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
  ariaExpanded,
  ariaPressed,
}: {
  variant: "close" | "collapse" | "wide";
  title: string;
  onClick: () => void;
  glyph: React.ReactNode;
  disabled?: boolean;
  /** 折叠按钮：展开/折叠是开合关系，用 aria-expanded */
  ariaExpanded?: boolean;
  /**
   * 宽度按钮：标准/阅读宽是二档切换，用 aria-pressed。
   *
   * 这里**不是** radiogroup + aria-checked：radiogroup 要求组内多个
   * radio 平铺，每个档位一个可聚焦控件；本控件是单个按钮在两组状态间
   * 来回切（WAI-ARIA 的 toggle button 模式），aria-pressed 表达的正是
   * 「这个按钮当前是否处于按下态」。
   *
   * 何时该改：档位增加到 3 个以上时，单按钮无法承载 N 选 1，必须拆成
   * radiogroup + 多个 radio + aria-checked，否则用户只能靠反复点击
   * 试出所有档位。`TrafficLights.test.tsx` 里有一条测试锁定了当前契约，
   * 改动语义时会先失败，提醒同步更新读屏预期。
   */
  ariaPressed?: boolean;
}) {
  // 字形按语义键重挂载：折叠 ↔ 展开、标准 ↔ 阅读宽 的图标互换
  // 由 CSS 动画交叉淡入，而不是硬切（硬切在 150ms 的 hover 淡入
  // 旁边显得突兀，像闪了一下）
  //
  // aria-label 已经写明动作（"折叠"/"展开"），aria-expanded / aria-pressed
  // 补的是**当前状态**：屏幕阅读器只读 label 时，用户无法知道现在是展开
  // 还是折叠、是标准宽还是阅读宽。
  return (
    <button
      type="button"
      aria-label={title}
      title={title}
      aria-expanded={ariaExpanded}
      aria-pressed={ariaPressed}
      onClick={onClick}
      disabled={disabled}
      className={`traffic-light traffic-light--${variant}`}
    >
      <svg key={title} viewBox="0 0 12 12" fill="none" aria-hidden="true">
        {glyph}
      </svg>
    </button>
  );
}
