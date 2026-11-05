declare const inventory: {
  commit: string
  runUrl: string
  members: {
    id: string
    scope: string
    purpose: string
    na?: Record<string, string>
    archived: Record<string, {
      status: string
      reason: string
      assertion?: string
    }>
  }[]
}

export default inventory
