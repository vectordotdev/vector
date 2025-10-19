#!/usr/bin/env python3
"""
NetFlow Template Inspector

This script listens on a UDP port and inspects incoming NetFlow/IPFIX templates
to help debug template parsing issues and understand the data structure.
"""

import socket
import struct
import json
import time
from datetime import datetime
from typing import Dict, List, Any

class NetFlowTemplateInspector:
    def __init__(self, port: int = 9995, bind_addr: str = "0.0.0.0"):
        self.port = port
        self.bind_addr = bind_addr
        self.sock = None
        self.template_stats = {}
        self.field_type_stats = {}
        
    def start_listening(self):
        """Start listening for NetFlow packets"""
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.bind((self.bind_addr, self.port))
        print(f"🔍 Listening for NetFlow packets on {self.bind_addr}:{self.port}")
        print("Press Ctrl+C to stop and see analysis\n")
        
        try:
            while True:
                data, addr = self.sock.recvfrom(65535)
                self.process_packet(data, addr)
        except KeyboardInterrupt:
            print("\n\n📊 Analysis Summary:")
            self.print_analysis()
        finally:
            if self.sock:
                self.sock.close()
    
    def process_packet(self, data: bytes, addr: tuple):
        """Process a NetFlow packet"""
        if len(data) < 4:
            return
            
        # Check if it's IPFIX (version 10)
        version = struct.unpack('>H', data[0:2])[0]
        
        if version == 10:  # IPFIX
            self.process_ipfix_packet(data, addr)
        elif version == 5:  # NetFlow v5
            self.process_netflow_v5_packet(data, addr)
        elif version == 9:  # NetFlow v9
            self.process_netflow_v9_packet(data, addr)
        else:
            print(f"❓ Unknown version {version} from {addr}")
    
    def process_ipfix_packet(self, data: bytes, addr: tuple):
        """Process IPFIX packet"""
        if len(data) < 16:
            return
            
        # Parse IPFIX header
        version, length, timestamp, seq_num, obs_domain = struct.unpack('>HHIII', data[0:16])
        
        print(f"📦 IPFIX packet from {addr}: length={length}, domain={obs_domain}")
        
        # Parse sets
        offset = 16
        while offset + 4 <= len(data) and offset < length:
            set_id, set_length = struct.unpack('>HH', data[offset:offset+4])
            
            if set_length < 4:
                break
                
            set_data = data[offset+4:offset+set_length]
            
            if set_id == 2:  # Template set
                self.analyze_template_set(set_data, addr, obs_domain)
            elif set_id == 3:  # Options template set
                self.analyze_options_template_set(set_data, addr, obs_domain)
            elif set_id >= 256:  # Data set
                print(f"  📊 Data set {set_id} (length: {set_length-4})")
            
            offset += set_length
    
    def analyze_template_set(self, data: bytes, addr: tuple, obs_domain: int):
        """Analyze template set"""
        offset = 0
        while offset + 4 <= len(data):
            template_id, field_count = struct.unpack('>HH', data[offset:offset+4])
            
            print(f"  🔧 Template {template_id}: {field_count} fields")
            
            # Track template stats
            key = (addr, obs_domain, template_id)
            if key not in self.template_stats:
                self.template_stats[key] = {
                    'count': 0,
                    'field_count': field_count,
                    'fields': []
                }
            self.template_stats[key]['count'] += 1
            
            # Parse fields
            field_offset = offset + 4
            fields = []
            
            for i in range(field_count):
                if field_offset + 4 > len(data):
                    print(f"    ⚠️  Incomplete field data at position {field_offset}")
                    break
                    
                field_type, field_length = struct.unpack('>HH', data[field_offset:field_offset+4])
                
                # Check for enterprise field
                enterprise_number = None
                if field_type & 0x8000:  # Enterprise bit set
                    if field_offset + 8 <= len(data):
                        enterprise_number = struct.unpack('>I', data[field_offset+4:field_offset+8])[0]
                        field_offset += 8
                    else:
                        print(f"    ⚠️  Enterprise field incomplete for type {field_type}")
                        field_offset += 4
                else:
                    field_offset += 4
                
                field_info = {
                    'type': field_type,
                    'length': field_length,
                    'enterprise': enterprise_number,
                    'enterprise_bit': bool(field_type & 0x8000)
                }
                
                fields.append(field_info)
                
                # Track field type stats
                if field_type not in self.field_type_stats:
                    self.field_type_stats[field_type] = {
                        'count': 0,
                        'lengths': [],
                        'enterprise_count': 0
                    }
                
                self.field_type_stats[field_type]['count'] += 1
                self.field_type_stats[field_type]['lengths'].append(field_length)
                if enterprise_number:
                    self.field_type_stats[field_type]['enterprise_count'] += 1
                
                # Flag suspicious lengths
                if field_length == 65535:
                    print(f"    🚨 Field type {field_type}: length 65535 (variable length)")
                elif field_length > 1000:
                    print(f"    ⚠️  Field type {field_type}: large length {field_length}")
                
                # Print field details
                enterprise_str = f" (enterprise: {enterprise_number})" if enterprise_number else ""
                print(f"    📋 Field {i+1}: type={field_type}, length={field_length}{enterprise_str}")
            
            # Store fields for this template
            self.template_stats[key]['fields'] = fields
            
            # Move to next template
            offset = field_offset
    
    def analyze_options_template_set(self, data: bytes, addr: tuple, obs_domain: int):
        """Analyze options template set"""
        print(f"  ⚙️  Options template set (length: {len(data)})")
    
    def process_netflow_v5_packet(self, data: bytes, addr: tuple):
        """Process NetFlow v5 packet"""
        print(f"📦 NetFlow v5 packet from {addr}")
    
    def process_netflow_v9_packet(self, data: bytes, addr: tuple):
        """Process NetFlow v9 packet"""
        print(f"📦 NetFlow v9 packet from {addr}")
    
    def print_analysis(self):
        """Print analysis summary"""
        print(f"\n📈 Template Statistics:")
        print(f"  Total unique templates: {len(self.template_stats)}")
        
        # Most common templates
        sorted_templates = sorted(self.template_stats.items(), 
                                key=lambda x: x[1]['count'], reverse=True)
        
        print(f"\n🔝 Most Common Templates:")
        for (addr, domain, template_id), stats in sorted_templates[:10]:
            print(f"  Template {template_id} from {addr}: {stats['count']} times")
        
        # Field type analysis
        print(f"\n📊 Field Type Analysis:")
        sorted_field_types = sorted(self.field_type_stats.items(), 
                                  key=lambda x: x[1]['count'], reverse=True)
        
        for field_type, stats in sorted_field_types[:20]:
            avg_length = sum(stats['lengths']) / len(stats['lengths'])
            enterprise_pct = (stats['enterprise_count'] / stats['count']) * 100
            print(f"  Type {field_type}: {stats['count']} occurrences, "
                  f"avg length {avg_length:.1f}, "
                  f"{enterprise_pct:.1f}% enterprise")
            
            # Show length distribution
            lengths = stats['lengths']
            unique_lengths = sorted(set(lengths))
            if len(unique_lengths) <= 5:
                print(f"    Lengths: {unique_lengths}")
            else:
                print(f"    Lengths: {unique_lengths[:3]}...{unique_lengths[-2:]}")
        
        # Problematic field types
        print(f"\n🚨 Problematic Field Types:")
        for field_type, stats in sorted_field_types:
            lengths = stats['lengths']
            if 65535 in lengths or max(lengths) > 1000:
                print(f"  Type {field_type}: max length {max(lengths)}, "
                      f"65535 count: {lengths.count(65535)}")

def main():
    import argparse
    
    parser = argparse.ArgumentParser(description='Inspect NetFlow templates')
    parser.add_argument('--port', type=int, default=9995, help='UDP port to listen on')
    parser.add_argument('--bind', default='0.0.0.0', help='Address to bind to')
    parser.add_argument('--duration', type=int, help='Run for N seconds then exit')
    
    args = parser.parse_args()
    
    inspector = NetFlowTemplateInspector(port=args.port, bind_addr=args.bind)
    
    if args.duration:
        print(f"⏱️  Running for {args.duration} seconds...")
        import signal
        import sys
        
        def signal_handler(sig, frame):
            print(f"\n⏰ Time's up! ({args.duration}s elapsed)")
            inspector.print_analysis()
            sys.exit(0)
        
        signal.signal(signal.SIGALRM, signal_handler)
        signal.alarm(args.duration)
    
    inspector.start_listening()

if __name__ == "__main__":
    main()
