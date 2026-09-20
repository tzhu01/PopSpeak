import { Component, type ReactNode, type ErrorInfo } from 'react'
import { TitleBar } from './MainLayout/TitleBar'

interface Props {
  children: ReactNode
}

interface State {
  hasError: boolean
  error: Error | null
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props)
    this.state = { hasError: false, error: null }
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('ErrorBoundary caught:', error, info.componentStack)
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="brand-shell flex flex-col h-screen font-sans text-text-primary">
          {!['#capsule', '#editor'].includes(window.location.hash) && <TitleBar />}
          <div className="p-8 flex flex-1 flex-col items-center justify-center">
            <h2 className="mb-2">页面暂时无法显示</h2>
            <p className="text-text-secondary mb-4">
              {this.state.error?.message || 'An unexpected error occurred.'}
            </p>
            <button
              onClick={() => window.location.reload()}
              className="px-6 py-2 rounded-[6px] border border-border bg-bg-secondary cursor-pointer text-sm"
            >
              重新加载
            </button>
          </div>
        </div>
      )
    }

    return this.props.children
  }
}
