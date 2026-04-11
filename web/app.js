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
            // 5秒后重连
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
                
            case 'content':
                this.appendContent(data.text);
                break;
                
            case 'tool_call':
                this.showToolCall(data.name);
                break;
                
            case 'tool_result':
                this.showToolResult(data.name, data.size);
                break;
                
            case 'loop_progress':
                this.showLoopProgress(data.current, data.max);
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
        }
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
            // 使用单独的内容区域，避免覆盖工具指示器
            let contentArea = bubble.querySelector('.response-content');
            if (!contentArea) {
                contentArea = document.createElement('div');
                contentArea.className = 'response-content';
                bubble.insertBefore(contentArea, bubble.firstChild);
            }
            contentArea.innerHTML = this.formatContent(this.currentResponse);
            this.scrollToBottom();
        }
    }
    
    finishResponse() {
        const bubble = document.getElementById('streaming-bubble');
        if (bubble) {
            bubble.removeAttribute('id');
        }
        this.currentResponse = '';
    }
    
    showToolCall(name) {
        const bubble = document.getElementById('streaming-bubble');
        if (bubble) {
            const indicator = document.createElement('div');
            indicator.className = 'tool-indicator';
            indicator.innerHTML = `
                <span class="spinner"></span>
                <span>调用工具: ${name}</span>
            `;
            bubble.appendChild(indicator);
            this.scrollToBottom();
        }
    }
    
    showToolResult(name, size) {
        const bubble = document.getElementById('streaming-bubble');
        if (bubble) {
            const result = document.createElement('div');
            result.className = 'tool-indicator';
            result.innerHTML = `<span>✓ ${name}: ${size} 字节</span>`;
            bubble.appendChild(result);
            this.scrollToBottom();
        }
    }
    
    showLoopProgress(current, max) {
        const bubble = document.getElementById('streaming-bubble');
        if (bubble) {
            const progress = document.createElement('div');
            progress.className = 'tool-indicator';
            progress.innerHTML = `<span>── 自动执行 (第 ${current}/${max} 轮) ──</span>`;
            bubble.appendChild(progress);
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
        
        // 转義 HTML
        let formatted = content
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;');
        
        // 代码块
        formatted = formatted.replace(/```(\w*)\n([\s\S]*?)```/g, (_, lang, code) => {
            return `<pre><code class="language-${lang}">${code.trim()}</code></pre>`;
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
