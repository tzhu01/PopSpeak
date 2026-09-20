const header = document.querySelector('[data-header]');
const navToggle = document.querySelector('.nav-toggle');
const nav = document.querySelector('#site-nav');
const liveText = document.querySelector('[data-live-text]');
const liveState = document.querySelector('[data-live-state]');
const liveNote = document.querySelector('[data-live-note]');
const copyButton = document.querySelector('[data-copy-demo]');
const layoutButton = document.querySelector('[data-layout-toggle]');
const autoCopy = document.querySelector('[data-auto-copy]');
const toast = document.querySelector('[data-toast]');

const demoFrames = [
  {
    state: '正在录音 · 近似预览',
    text: '我想把今天讨论的三个重点整理出来。',
    note: '最终结果将由所选模型校正',
  },
  {
    state: '正在录音 · 持续修订',
    text: '我想把今天讨论的三个重点整理出来：产品定位、发布节奏，还有用户反馈。',
    note: '本地辅路正在更新近似文字',
  },
  {
    state: '识别完成 · 最终结果',
    text: '请把今天讨论的三个重点整理出来：产品定位、发布节奏，以及用户反馈。',
    note: '最终模型已校正并接管结果',
  },
];

let frameIndex = 0;
let layoutMode = 0;
let toastTimer;
let demoTimer;
const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

function setHeaderState() {
  header?.classList.toggle('scrolled', window.scrollY > 16);
}

function setFrame(index) {
  const frame = demoFrames[index];
  if (!frame || !liveText || !liveState || !liveNote) return;
  liveText.animate(
    [{ opacity: 0.45, transform: 'translateY(4px)' }, { opacity: 1, transform: 'translateY(0)' }],
    { duration: 260, easing: 'ease-out' },
  );
  liveState.textContent = frame.state;
  liveText.textContent = frame.text;
  liveNote.textContent = frame.note;
  if (index === demoFrames.length - 1 && autoCopy?.checked) {
    autoCopy.checked = false;
    void writeClipboard(frame.text).then((copied) => {
      showToast(copied ? '最终文字已自动复制' : '自动复制未获浏览器授权');
    });
  }
}

function showToast(message) {
  if (!toast) return;
  toast.textContent = message;
  toast.classList.add('visible');
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => toast.classList.remove('visible'), 1800);
}

async function writeClipboard(value) {
  try {
    await navigator.clipboard.writeText(value);
    return true;
  } catch {
    const helper = document.createElement('textarea');
    helper.value = value;
    helper.setAttribute('readonly', '');
    helper.style.position = 'fixed';
    helper.style.opacity = '0';
    document.body.appendChild(helper);
    helper.select();
    const copied = document.execCommand('copy');
    helper.remove();
    return copied;
  }
}

async function copyDemoText() {
  const value = liveText?.textContent?.trim();
  if (!value) return;
  const copied = await writeClipboard(value);
  showToast(copied ? '文字已复制' : '请长按文字进行复制');
}

navToggle?.addEventListener('click', () => {
  const expanded = navToggle.getAttribute('aria-expanded') === 'true';
  navToggle.setAttribute('aria-expanded', String(!expanded));
  nav?.classList.toggle('open', !expanded);
  const label = navToggle.querySelector('.sr-only');
  if (label) label.textContent = expanded ? '打开导航' : '关闭导航';
});

nav?.querySelectorAll('a').forEach((link) => {
  link.addEventListener('click', () => {
    nav?.classList.remove('open');
    navToggle?.setAttribute('aria-expanded', 'false');
    const label = navToggle?.querySelector('.sr-only');
    if (label) label.textContent = '打开导航';
  });
});

copyButton?.addEventListener('click', copyDemoText);
layoutButton?.addEventListener('click', () => {
  const modes = ['排版：自动', '排版：去换行', '排版：多行'];
  layoutMode = (layoutMode + 1) % modes.length;
  layoutButton.textContent = modes[layoutMode];
  showToast(`已切换为${modes[layoutMode].replace('排版：', '')}排版`);
});

document.querySelectorAll('.icon-button').forEach((button) => {
  button.addEventListener('click', () => {
    button.classList.toggle('pin-active');
    const active = button.classList.contains('pin-active');
    const label = active ? '窗口已置顶' : '窗口未置顶';
    button.setAttribute('aria-pressed', String(active));
    button.setAttribute('aria-label', label);
    button.setAttribute('title', label);
    showToast(active ? '结果窗已置顶' : '已取消置顶');
  });
});

const revealObserver = new IntersectionObserver(
  (entries) => entries.forEach((entry) => entry.isIntersecting && entry.target.classList.add('visible')),
  { threshold: 0.12 },
);

document.querySelectorAll('[data-reveal]').forEach((element) => revealObserver.observe(element));
document.querySelectorAll('[data-year]').forEach((element) => { element.textContent = new Date().getFullYear(); });

window.addEventListener('scroll', setHeaderState, { passive: true });
setHeaderState();

function stopDemo() {
  if (!demoTimer) return;
  window.clearInterval(demoTimer);
  demoTimer = undefined;
}

function startDemo() {
  if (demoTimer || document.hidden || reduceMotion.matches) return;
  demoTimer = window.setInterval(() => {
    frameIndex = (frameIndex + 1) % demoFrames.length;
    setFrame(frameIndex);
  }, 3200);
}

function syncDemoMotion() {
  if (document.hidden || reduceMotion.matches) {
    stopDemo();
    if (reduceMotion.matches) {
      frameIndex = demoFrames.length - 1;
      setFrame(frameIndex);
    }
    return;
  }
  startDemo();
}

document.addEventListener('visibilitychange', syncDemoMotion);
reduceMotion.addEventListener?.('change', syncDemoMotion);
setFrame(frameIndex);
syncDemoMotion();
