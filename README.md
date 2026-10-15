# Schema Compare — 1.0.0-beta

โปรแกรม Windows สำหรับเปรียบเทียบโครงสร้าง SQL Server ระหว่าง Source และ Target พร้อม native GUI และ CLI เขียนด้วย Rust ใช้งานแบบ Portable ZIP ไม่มีระบบ license, activation หรือ licensing server

Repository: [jirakitc/MSSQL-Schema-compare](https://github.com/jirakitc/MSSQL-Schema-compare) · [GitHub Releases](https://github.com/jirakitc/MSSQL-Schema-compare/releases)

## Release 1.0.0-beta

รุ่น beta นี้รวมความสามารถปัจจุบันไว้ใน release เดียว:

- เชื่อมต่อและอ่าน Source/Target พร้อมกัน พร้อมแสดงสถานะการทำงาน
- Compare แบบแบ่งกลุ่มตามชนิด object หรือชนิดความต่าง มี SQL Differences สองฝั่งพร้อมเลขบรรทัดและสีไฮไลต์
- Different / Source only / Target only เป็น multi-select filters; เลือกแถวตามสถานะ ชนิด object และ Search objects พร้อมยกเลิกแถวที่ถูกซ่อน
- Edit project ขนาดกะทัดรัด ลากย้ายได้ และเลือก Database จาก dropdown ที่โหลดชื่อฐานเมื่อเปิด
- Recent connections เก็บ 12 คู่ล่าสุดที่ Compare สำเร็จ แบ่งกลุ่มตาม Source Address → Target Address มี Search, Use connection และ Clear โดยไม่เก็บ password ลงไฟล์
- สร้างและรัน deployment script เฉพาะ object ที่เลือก ตรวจ schema drift ก่อนรัน และ Compare ใหม่หลัง Deploy พร้อมแสดง Success/Fail
- ยึดโครงสร้างคอลัมน์ Source สำหรับตารางที่เลือก: เพิ่มคอลัมน์ ลบคอลัมน์ส่วนเกิน และเปลี่ยน datatype รวมการลดขนาด โดยตรวจข้อมูลจริงก่อนแปลง
- สร้าง index/PK/UNIQUE/FK/CHECK/default ที่เกี่ยวข้องกลับภายใน transaction; เมื่อเกิด error จะหยุดและ rollback
- Help → About แสดงเวอร์ชันปัจจุบันและ GitHub release ที่เผยแพร่ล่าสุด รวม pre-release; ปุ่ม Update เปิดหน้า GitHub Releases เพื่อดาวน์โหลด ZIP ด้วยตนเอง
- ไอคอนโปรแกรมและเวอร์ชันใน GUI, CLI และ Windows EXE ตรงกันเป็น 1.0.0-beta

## เริ่มใช้งาน

1. แตก `SchemaCompare-1.0.0-beta-win-x64.zip` แล้วเปิด `schema-compare.exe`
2. กด **Edit project** หรือ **Project → Edit project** แล้วกำหนด Source และ Target
3. กรอก Server, Authentication และ Username/Password ตามการเชื่อมต่อ เปิด dropdown Database เพื่อเลือกฐาน
4. กด **Compare now / Refresh** โปรแกรมอ่าน catalog โดยยังไม่แก้ฐานข้อมูล
5. เลือกสถานะและ object ที่ต้องการ ตรวจ **SQL Differences** และ **Deployment → Review SQL**
6. กด **Deploy** แล้วตรวจ Target และสคริปต์ก่อนยืนยันการรัน

Source คือโครงสร้างที่ต้องการให้ Target เป็น เมื่อเลือก migration ที่ลบคอลัมน์ ข้อมูลในคอลัมน์ที่ลบจะหาย ชื่อคอลัมน์ที่ต่างกันถือเป็น drop/add และไม่คัดลอกค่าให้อัตโนมัติ

การลดขนาด datatype ใช้การตรวจ conversion และ round-trip หากข้อความยาวเกิน ข้อมูลแปลงไม่ได้ หรือค่าจะถูกปัดเศษจนสูญเสียข้อมูล จะหยุดและ rollback แทนการตัดค่าเงียบ ๆ

## เมนู

| หมวด | คำสั่ง |
| --- | --- |
| Project | New, Open, Save, Edit, Recent connections |
| Compare | Compare/Refresh, Comparison results, Ignore options, Filter, Swap |
| Deployment | Review SQL, Deploy |
| Export | Comparison report, SQL script, Source/Target snapshot |
| Help | About, Update (GitHub Releases) |

## การตรวจเวอร์ชันและ Update

เปิด **Help → About** เพื่อดู Installed version และ Latest GitHub release โปรแกรมตรวจ GitHub แบบ background เพื่อไม่ให้หน้าจอค้าง มีปุ่ม **Check again** สำหรับลองใหม่ หากยังไม่มี release จะบอกว่าไม่มี release เผยแพร่ หากเครือข่ายผิดพลาดจะบอกว่าตรวจไม่สำเร็จ

ปุ่ม **Update** เปิด [หน้า Releases](https://github.com/jirakitc/MSSQL-Schema-compare/releases) ใน browser ไม่ดาวน์โหลด ติดตั้ง หรือเปลี่ยนไฟล์โปรแกรมอัตโนมัติ การตรวจเวอร์ชันส่งเฉพาะ application version ใน User-Agent ไม่ส่ง server/database/login/password หรือ Recent history

## ขอบเขตที่รองรับ

- ตารางและคอลัมน์ รวม default, computed และ identity metadata
- SQL views, stored procedures, functions และ DML triggers
- Rowstore indexes, PRIMARY KEY, UNIQUE, FOREIGN KEY และ CHECK constraints
- Schemas และ synonyms ในขอบเขตที่ catalog รองรับ
- Live SQL Server และ JSON snapshots สำหรับ Source/Target
- Ignore options สำหรับ comments, module SET options, collation, column order/case, default constraint names, indexes, fill factor, foreign keys, checks และ triggers

ยังไม่ครอบคลุม permissions/users/roles, extended properties, sequences, CLR, full-text, physical filegroups และ database options การเปลี่ยน identity/computed/default/collation, schema-bound dependencies, standalone user-created statistics และการสลับลำดับคอลัมน์อาจต้อง migration เพิ่มเติม SQL Server ที่มี advanced table/index features บางประเภทจะถูกปฏิเสธการ capture พร้อมเหตุผล

## การจัดเก็บข้อมูล

Recent history อยู่ที่ `%LOCALAPPDATA%\SchemaCompare\recent-connections.json` เก็บ connection settings และ project options โดยไม่บันทึก password ปุ่ม **Clear** ล้างเฉพาะรายการย้อนหลังและไม่เปลี่ยน project ปัจจุบัน Native project JSON ไม่บันทึก password เช่นกัน

## ความต้องการของเครื่อง

- Windows x64
- Microsoft SQL Server command-line utilities แบบ ODBC (`sqlcmd`) สำหรับการเชื่อมต่อฐานจริง
- สิทธิ์อ่าน catalog และ `VIEW DEFINITION` เพื่อ Compare; สิทธิ์ DDL ตาม object ที่จะ Deploy
- Internet และ Windows HTTPS client (`curl.exe`) สำหรับตรวจ GitHub release; Compare/Deploy ไม่ต้องเชื่อมต่อ GitHub

## Build และตรวจสอบ

```powershell
cargo fmt --all
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
.\build.ps1 -OutputDirectory 'dist\SchemaCompare-1.0.0-beta'
```

Build ใช้ Rust และ MinGW-w64 GNU toolchain ที่ตั้งไว้ใน `.cargo/config.toml` หรือปรับเป็น MSVC toolchain ตามเครื่อง คำสั่ง build สร้าง GUI/CLI, icon assets, README และ samples สำหรับจัดเป็น Portable ZIP พร้อม third-party license notices

`test.ps1 -Integration` ทดสอบบน LocalDB instance ชื่อสุ่มแยกจากฐานจริง ครอบคลุม capture, Compare, Deploy, schema drift, data preservation และ rollback หลักฐานเก็บใน `test-results` และไม่รวมในชุดแจกจ่าย

ดู `THIRD_PARTY_NOTICES.md` และ `third-party-licenses` สำหรับ notices ของ dependency ที่รวมใน ZIP
