// xflow Web 前端

class XflowApp {
    constructor() {
        this.ws = null;
        this.sessionId = null;
        this.messages = [];
        this.connected = false;
        
        this.elements = {
            connectionStatus: document.getElementById('connectionStatus'),
            modelInfo: document.getElementById('modelInfo'),
            newChatBtn: document.getElementById('newChatBtn'),
            historyList: document.getElementById('historyList'),
            chatContainer: document.getElementById('chatContainer'),
            welcomeScreen: document.getElementById('welcomeScreen'),
            messages: document.getElementById('messages'),
            messageInput: document.getElementById('messageInput'),
            sendBtn: document.getElementById('sendBtn'),
            charCount: document.getElementById('charCount'),
        };
        
        this.init();
    }
    
    init() {
        this.bindEvents();
        this.connect();
    }
    
    bindEvents() {
        // 发送按钮
        this.elements.sendBtn.addEventListener('click', () => this.sendMessage());
        
        // 输入框
        this.elements.messageInput.addEventListener('input', (e) => {
            this.autoResizeTextarea(e.target);
            this.updateCharCount();
        });
        
        this.elements.messageInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                this.sendMessage();
            }
        });
        
        // 新对话
        this.elements.newChatBtn.addEventListener('click', () => this.newChat());
        
        // 快捷操作
        document.querySelectorAll('.quick-action').forEach(btn => {
            btn.addEventListener('click', () => {
                const prompt = btn.dataset.prompt;
                this.elements.messageInput.value = prompt;
                this.sendMessage();
            });
        });
    }
    
    connect() {
        this.updateConnectionStatus('connecting');
        
        const wsUrl = this.getWsUrl();
        this.ws = new WebSocket(wsUrl);
        
        this.ws.onopen = () => {
            console.log('WebSocket 连接成功');
            this.connected = true;
            this.updateConnectionStatus('connected');
        };
        
        this.ws.onclose = () => {
            console.log('WebSocket 连接关闭');
            this.connected = false;
            this.updateConnectionStatus('disconnected');
            // 5秒后重连（保持 session_id）
            setTimeout(() => this.connect(), 5000);
        };
        
        this.ws.onerror = (error) => {
            console.error('WebSocket 错误:', error);
            this.updateConnectionStatus('disconnected');
        };
        
        this.ws.onmessage = (event) => {
            this.handleMessage(JSON.parse(event.data));
        };
    }
    
    getWsUrl() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        return `${protocol}//${window.location.host}/api/ws`;
    }
    
    handleMessage(data) {
        console.log('收到消息:', data);
        
        switch (data.type) {
            case 'session_info':
                this.sessionId = data.session_id;
                console.log('会话ID:', this.sessionId);
                break;
                
            case 'thinking':
                this.showThinking();
                break;
                
            case 'thinking_dot':
                this.updateThinkingDots();
                break;
                
            case 'thinking_content':
                this.appendThinkingContent(data.text);
                break;
                
            case 'content':
                this.appendContent(data.text);
                break;
                
            case 'tool_call':
                this.showToolCall(data.name, data.params_display);
                break;
                
            case 'tool_result':
                this.showToolResult(data.name, data.result, data.size, data.success);
                break;
                
            case 'loop_progress':
                // 不显示循环进度
                break;
                
            case 'done':
                this.finishResponse();
                break;
                
            case 'error':
                this.showError(data.message);
                break;
                
            case 'pong':
                // 心跳响应，忽略
                break;
                
            case 'confirmation_request':
                this.handleConfirmationRequest(data);
                break;
        }
    }
    
    // 处理确认请求
    handleConfirmationRequest(data) {
        const { id, tool, message, danger_level, danger_reason } = data;
        
        // 显示确认对话框
        this.showConfirmationDialog({
            id,
            tool,
            message,
            dangerLevel: danger_level,
            dangerReason: danger_reason
        });
    }
    
    // 显示确认对话框
    showConfirmationDialog(request) {
        // 创建对话框元素
        const dialog = document.createElement('div');
        dialog.className = 'confirmation-dialog';
        dialog.id = `dialog-${request.id}`;
        
        // 根据危险级别设置样式
        const dangerEmoji = {
            0: '⚠️',
            1: '🟡',
            2: '🟠',
            3: '🔴'
        };
        
        const dangerText = {
            0: '需要确认',
            1: '中度危险',
            2: '高度危险',
            3: '极度危险'
        };
        
        const level = request.dangerLevel || 0;
        
        dialog.innerHTML = `
            <div class="dialog-backdrop"></div>
            <div class="dialog-content ${level >= 2 ? 'danger' : ''}">
                <div class="dialog-header">
                    <span class="danger-indicator">${dangerEmoji[level] || '⚠️'}</span>
                    <h3>${dangerText[level] || '需要确认'}</h3>
                </div>
                <div class="dialog-body">
                    <div class="tool-name">工具: <code>${this.escapeHtml(request.tool)}</code></div>
                    <div class="tool-message">
                        <pre>${this.escapeHtml(request.message)}</pre>
                    </div>
                    ${request.dangerReason ? `
                        <div class="danger-reason">
                            <strong>原因:</strong> ${this.escapeHtml(request.dangerReason)}
                        </div>
                    ` : ''}
                </div>
                <div class="dialog-actions">
                    <button class="btn-cancel" id="cancel-${request.id}">取消</button>
                    <button class="btn-confirm ${level >= 2 ? 'danger' : ''}" id="confirm-${request.id}">
                        ${level >= 2 ? '强制执行' : '确认执行'}
                    </button>
                </div>
            </div>
        `;
        
        document.body.appendChild(dialog);
        
        // 绑定按钮事件
        const confirmBtn = document.getElementById(`confirm-${request.id}`);
        const cancelBtn = document.getElementById(`cancel-${request.id}`);
        const backdrop = dialog.querySelector('.dialog-backdrop');
        
        const sendResponse = (approved) => {
            // 发送响应
            this.ws.send(JSON.stringify({
                type: 'confirmation_response',
                id: request.id,
                approved: approved
            }));
            
            // 关闭对话框
            dialog.remove();
        };
        
        confirmBtn.addEventListener('click', () => sendResponse(true));
        cancelBtn.addEventListener('click', () => sendResponse(false));
        backdrop.addEventListener('click', () => sendResponse(false));
        
        // 键盘事件
        const handleKeydown = (e) => {
            if (e.key === 'Escape') {
                sendResponse(false);
                document.removeEventListener('keydown', handleKeydown);
            } else if (e.key === 'Enter' && e.ctrlKey) {
                sendResponse(true);
                document.removeEventListener('keydown', handleKeydown);
            }
        };
        document.addEventListener('keydown', handleKeydown);
        
        // 聚焦确认按钮
        confirmBtn.focus();
    }
    
    // HTML 转义
    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
    
    sendMessage() {
        const text = this.elements.messageInput.value.trim();
        if (!text || !this.connected) return;
        
        // 隐藏欢迎屏幕
        this.elements.welcomeScreen.classList.add('hidden');
        
        // 添加用户消息
        this.addMessage('user', text);
        
        // 清空输入框
        this.elements.messageInput.value = '';
        this.autoResizeTextarea(this.elements.messageInput);
        this.updateCharCount();
        
        // 发送到服务器
        this.ws.send(JSON.stringify({
            type: 'chat',
            message: text
        }));
        
        // 准备接收响应
        this.currentResponse = '';
        this.thinkingContent = '';
        this.toolIndicators = [];  // 工具指示器列表
        this.addMessage('assistant', '', true);
    }
    
    addMessage(role, content, streaming = false) {
        const message = document.createElement('div');
        message.className = `message ${role}`;
        
        const avatar = document.createElement('div');
        avatar.className = 'message-avatar';
        avatar.textContent = role === 'user' ? '👤' : '⚡';
        
        const contentDiv = document.createElement('div');
        contentDiv.className = 'message-content';
        
        const bubble = document.createElement('div');
        bubble.className = 'message-bubble';
        bubble.innerHTML = this.formatContent(content);
        
        if (streaming) {
            bubble.id = 'streaming-bubble';
        }
        
        contentDiv.appendChild(bubble);
        message.appendChild(avatar);
        message.appendChild(contentDiv);
        
        this.elements.messages.appendChild(message);
        this.scrollToBottom();
        
        return message;
    }
    
    appendContent(text) {
        const bubble = document.getElementById('streaming-bubble');
        if (bubble) {
            this.currentResponse += text;
            
            // 检查是否已创建响应内容区域
            let contentArea = bubble.querySelector('.response-content');
            if (!contentArea && text.trim()) {
                // 如果有思考内容或工具调用，添加分隔
                const thinkingContent = bubble.querySelector('.thinking-content');
                const toolIndicators = bubble.querySelectorAll('.tool-indicator, .tool-result-status');
                if (thinkingContent || toolIndicators.length > 0) {
                    const spacer = document.createElement('div');
                    spacer.style.marginTop = '16px';
                    bubble.appendChild(spacer);
                }
                
                // 创建内容区域
                contentArea = document.createElement('div');
                contentArea.className = 'response-content';
                bubble.appendChild(contentArea);
            }
            
            // 将图标和内容在同一行显示
            if (contentArea) {
                contentArea.innerHTML = `<span class="response-icon"><span class="icon-purple">✦</span></span> ${this.formatContent(this.currentResponse)}`;
            }
            this.scrollToBottom();
        }
    }
    
    finishResponse() {
        const bubble = document.getElementById('streaming-bubble');
        if (bubble) {
            bubble.removeAttribute('id');
        }
        this.currentResponse = '';
        this.thinkingContent = '';
    }
    
    showToolCall(name, paramsDisplay) {
        const bubble = document.getElementById('streaming-bubble');
        if (bubble) {
            // 如果有思考内容，先添加换行
            const thinkingContent = bubble.querySelector('.thinking-content');
            if (thinkingContent && !bubble.querySelector('.tool-indicator')) {
                const spacer = document.createElement('div');
                spacer.style.marginTop = '8px';
                bubble.appendChild(spacer);
            }
            
            const indicator = document.createElement('div');
            indicator.className = 'tool-indicator';
            if (paramsDisplay) {
                indicator.innerHTML = `
                    <span class="tool-icon">🛠</span>
                    <span class="tool-name">${this.escapeHtml(name)}</span>
                    <span class="tool-params">${this.escapeHtml(paramsDisplay)}</span>
                `;
            } else {
                indicator.innerHTML = `
                    <span class="tool-icon">🛠</span>
                    <span class="tool-name">${this.escapeHtml(name)}</span>
                `;
            }
            bubble.appendChild(indicator);
            this.scrollToBottom();
        }
    }
    
    showToolResult(name, result, size, success) {
        const bubble = document.getElementById('streaming-bubble');
        if (bubble) {
            // 显示结果内容
            if (result && result.length > 0) {
                const resultContent = document.createElement('div');
                resultContent.className = 'tool-result-content';
                // 截断显示
                const displayResult = result.length > 500 ? result.substring(0, 500) + '...' : result;
                resultContent.textContent = displayResult;
                bubble.appendChild(resultContent);
            }
            
            // 显示状态
            const status = document.createElement('div');
            status.className = 'tool-result-status';
            
            const sizeStr = size > 1024 
                ? `${(size / 1024).toFixed(1)}KB` 
                : `${size}B`;
            
            if (success) {
                status.innerHTML = `<span class="status-success">✓</span> <span class="status-text">调用成功</span> <span class="status-size">(${sizeStr})</span>`;
            } else {
                status.innerHTML = `<span class="status-failure">✗</span> <span class="status-text">调用失败</span> <span class="status-size">(${sizeStr})</span>`;
            }
            bubble.appendChild(status);
            this.scrollToBottom();
        }
    }
    
    showThinking() {
        const bubble = document.getElementById('streaming-bubble');
        if (bubble) {
            this.thinkingContent = '';
            this.thinkingDotCount = 0;
            
            const hasContent = bubble.children.length > 0;
            
            const indicator = document.createElement('div');
            indicator.className = 'thinking-indicator';
            if (hasContent) {
                const spacer = document.createElement('div');
                spacer.style.marginTop = '12px';
                bubble.appendChild(spacer);
            }
            indicator.innerHTML = `<span class="thinking-icon">✻</span> <span class="thinking-text">Thinking...</span>`;
            bubble.appendChild(indicator);
            this.scrollToBottom();
        }
    }
    
    updateThinkingDots() {
        const bubble = document.getElementById('streaming-bubble');
        if (!bubble) return;
        
        const indicators = bubble.querySelectorAll('.thinking-indicator');
        const indicator = indicators.length > 0 ? indicators[indicators.length - 1] : null;
        if (!indicator) return;
        
        this.thinkingDotCount = ((this.thinkingDotCount || 0) + 1) % 4;
        const dots = '.'.repeat(this.thinkingDotCount);
        const textEl = indicator.querySelector('.thinking-text');
        if (textEl) {
            textEl.textContent = 'Thinking' + dots;
        }
    }
    
    appendThinkingContent(text) {
        const bubble = document.getElementById('streaming-bubble');
        if (bubble) {
            // 找到最后一个"思考中..."指示器（当前最新的）
            const indicators = bubble.querySelectorAll('.thinking-indicator');
            const indicator = indicators.length > 0 ? indicators[indicators.length - 1] : null;
            
            // 在指示器后面查找或创建思考内容区域
            let contentArea = null;
            if (indicator) {
                // 检查指示器的下一个兄弟是否是思考内容
                let nextSibling = indicator.nextElementSibling;
                if (nextSibling && nextSibling.classList.contains('thinking-content')) {
                    contentArea = nextSibling;
                } else {
                    // 在指示器后面创建新的思考内容区域
                    contentArea = document.createElement('div');
                    contentArea.className = 'thinking-content';
                    indicator.after(contentArea);
                }
            } else {
                // 没有指示器，直接追加到 bubble
                contentArea = document.createElement('div');
                contentArea.className = 'thinking-content';
                bubble.appendChild(contentArea);
            }
            
            // 追加内容
            this.thinkingContent = (this.thinkingContent || '') + text;
            contentArea.textContent = this.thinkingContent;
            this.scrollToBottom();
        }
    }
    
    showError(message) {
        const bubble = document.getElementById('streaming-bubble');
        if (bubble) {
            bubble.innerHTML += `<div style="color: var(--error);">错误: ${message}</div>`;
            bubble.removeAttribute('id');
        }
    }
    
    formatContent(content) {
        // 简单的 Markdown 格式化
        if (!content) return '';
        
        // 转義 HTML - 在 Markdown 处理前先转义
        let formatted = content
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;');
        
        // 代码块 - 转义语言标识符防止 XSS
        formatted = formatted.replace(/```(\w*)\n([\s\S]*?)```/g, (_, lang, code) => {
            // 语言标识符只允许字母数字和常见字符
            const safeLang = lang.replace(/[^a-zA-Z0-9+-]/g, '');
            return `<pre><code class="language-${safeLang}">${code.trim()}</code></pre>`;
        });
        
        // 行内代码
        formatted = formatted.replace(/`([^`]+)`/g, '<code>$1</code>');
        
        // 粗体
        formatted = formatted.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
        
        // 换行
        formatted = formatted.replace(/\n/g, '<br>');
        
        return formatted;
    }
    
    scrollToBottom() {
        this.elements.chatContainer.scrollTop = this.elements.chatContainer.scrollHeight;
    }
    
    autoResizeTextarea(textarea) {
        textarea.style.height = 'auto';
        textarea.style.height = textarea.scrollHeight + 'px';
    }
    
    updateCharCount() {
        const count = this.elements.messageInput.value.length;
        this.elements.charCount.textContent = count;
    }
    
    updateConnectionStatus(status) {
        const statusEl = this.elements.connectionStatus;
        const dot = statusEl.querySelector('.status-dot');
        const text = statusEl.querySelector('span:last-child');
        
        dot.className = 'status-dot ' + status;
        
        const statusText = {
            connected: '已连接',
            connecting: '连接中...',
            disconnected: '未连接'
        };
        
        text.textContent = statusText[status] || status;
    }
    
    newChat() {
        // 清空消息
        this.elements.messages.innerHTML = '';
        
        // 显示欢迎屏幕
        this.elements.welcomeScreen.classList.remove('hidden');
        
        // 创建新会话
        if (this.connected) {
            this.ws.send(JSON.stringify({ type: 'clear' }));
        }
    }
}

// 启动应用
document.addEventListener('DOMContentLoaded', () => {
    window.app = new XflowApp();
});
