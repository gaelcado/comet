#!/usr/bin/python3
"""Offline native app-server with WebRTC signaling and canonical voice events."""
import json, pathlib, sys, threading, time
root=pathlib.Path(__file__).resolve().parents[1]
session=None
thread='idle-thread'
wire_lock=threading.Lock()
def send(v):
    with wire_lock:print(json.dumps(v),flush=True)
def notify(method,params):send({'method':method,'params':dict(threadId=thread,**params)})
def identity_watch():
    while True:
        if (root/'identity-updated').exists():
            (root/'identity-updated').unlink()
            send({'method':'account/updated','params':{'authMode':'chatgpt'}})
        time.sleep(0.01)
threading.Thread(target=identity_watch,daemon=True).start()
for line in sys.stdin:
    frame=json.loads(line);method=frame.get('method')
    with (root/'voice-wire.jsonl').open('a') as out:out.write(json.dumps(frame)+'\n')
    if 'id' not in frame:continue
    result={}
    if method=='config/read':result={'config':{}}
    elif method=='account/read':result={'account':{'type':(root/'account-mode').read_text().strip() if (root/'account-mode').exists() else 'chatgpt'}}
    elif method=='account/rateLimits/read':result={'ordinaryUsageAllowed':True}
    elif method=='thread/realtime/listVoices':result={'voices':{'v1':['juniper','ember'],'v2':['alloy'],'defaultV1':'juniper','defaultV2':'alloy'}}
    elif method in ('thread/start','thread/resume'):result={'thread':{'id':thread,'turns':[]}}
    elif method=='turn/start':result={'turn':{'id':'text-turn'}}
    elif method=='thread/realtime/start':
        p=frame['params'];assert p['transport']=={'type':'webrtc','sdp':'fixture-offer'};assert p['version']=='v3';assert p['outputModality']=='audio';assert p['clientManagedHandoffs'] is False;assert p['includeStartupContext'] is True
        session=p['realtimeSessionId']
    elif method=='thread/realtime/appendAudio':raise RuntimeError('subscription voice must never append PCM')
    send({'id':frame['id'],'result':result})
    if method=='thread/realtime/start':
        notify('thread/realtime/started',{'realtimeSessionId':session,'version':'v3'})
        notify('thread/realtime/sdp',{'sdp':'fixture-answer'})
        for role,item,text in [('user','u1','Make a local Codex task'),('user','u1','Make a local Codex task'),('assistant','a1','I can do that.')]:
            notify('thread/realtime/item/transcript/delta',{'itemId':item,'delta':text})
            notify('thread/realtime/item/completed',{'item':{'type':'transcriptSegment','id':item,'realtimeSessionId':session,'role':role,'text':text}})
            notify('thread/realtime/transcript/done',{'role':role,'text':text})
        notify('turn/started',{'turn':{'id':'native-task'}})
        notify('item/agentMessage/delta',{'turnId':'native-task','itemId':'task-reply','delta':'Native task output'})
        notify('thread/realtime/item/completed',{'item':{'type':'bemItemPromoted','id':'promotion','realtimeSessionId':session,'turnId':'native-task','itemId':'task-reply','presentation':{'type':'wholeItem'}}})
    elif method=='thread/realtime/stop':notify('thread/realtime/closed',{'reason':'requested'})
    elif method=='turn/start':notify('turn/completed',{'turn':{'id':'text-turn','status':'completed'}})
