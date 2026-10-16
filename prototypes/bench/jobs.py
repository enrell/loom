"""Windows Job Objects via ctypes: run a command with every descendant in one
job, then read exact totals — CPU time across all processes (even short-lived
rustc/node children a sampler would miss), peak committed memory of the whole
job, and the live member pids."""
import ctypes
import ctypes.wintypes as wt
import subprocess

k32 = ctypes.WinDLL("kernel32", use_last_error=True)
ntdll = ctypes.WinDLL("ntdll")

CREATE_SUSPENDED = 0x00000004
JobObjectBasicAccountingInformation = 1
JobObjectBasicProcessIdList = 3
JobObjectExtendedLimitInformation = 9


class IO_COUNTERS(ctypes.Structure):
    _fields_ = [(n, ctypes.c_ulonglong) for n in (
        "ReadOperationCount", "WriteOperationCount", "OtherOperationCount",
        "ReadTransferCount", "WriteTransferCount", "OtherTransferCount")]


class BASIC_LIMIT(ctypes.Structure):
    _fields_ = [("PerProcessUserTimeLimit", ctypes.c_int64), ("PerJobUserTimeLimit", ctypes.c_int64),
                ("LimitFlags", wt.DWORD), ("MinimumWorkingSetSize", ctypes.c_size_t),
                ("MaximumWorkingSetSize", ctypes.c_size_t), ("ActiveProcessLimit", wt.DWORD),
                ("Affinity", ctypes.c_size_t), ("PriorityClass", wt.DWORD), ("SchedulingClass", wt.DWORD)]


class EXTENDED_LIMIT(ctypes.Structure):
    _fields_ = [("BasicLimitInformation", BASIC_LIMIT), ("IoInfo", IO_COUNTERS),
                ("ProcessMemoryLimit", ctypes.c_size_t), ("JobMemoryLimit", ctypes.c_size_t),
                ("PeakProcessMemoryUsed", ctypes.c_size_t), ("PeakJobMemoryUsed", ctypes.c_size_t)]


class ACCOUNTING(ctypes.Structure):
    _fields_ = [("TotalUserTime", ctypes.c_int64), ("TotalKernelTime", ctypes.c_int64),
                ("ThisPeriodTotalUserTime", ctypes.c_int64), ("ThisPeriodTotalKernelTime", ctypes.c_int64),
                ("TotalPageFaultCount", wt.DWORD), ("TotalProcesses", wt.DWORD),
                ("ActiveProcesses", wt.DWORD), ("TotalTerminatedProcesses", wt.DWORD)]


k32.CreateJobObjectW.restype = wt.HANDLE
k32.CreateJobObjectW.argtypes = [ctypes.c_void_p, wt.LPCWSTR]
k32.AssignProcessToJobObject.argtypes = [wt.HANDLE, wt.HANDLE]
k32.QueryInformationJobObject.argtypes = [wt.HANDLE, ctypes.c_int, ctypes.c_void_p, wt.DWORD, ctypes.c_void_p]
k32.TerminateJobObject.argtypes = [wt.HANDLE, wt.UINT]
k32.CloseHandle.argtypes = [wt.HANDLE]
ntdll.NtResumeProcess.argtypes = [wt.HANDLE]


class Job:
    """`Job.spawn(cmd)` starts `cmd` suspended, puts it in a fresh job, resumes it."""

    def __init__(self):
        self.handle = k32.CreateJobObjectW(None, None)
        if not self.handle:
            raise ctypes.WinError(ctypes.get_last_error())
        self.proc = None

    @classmethod
    def spawn(cls, cmd, **popen):
        job = cls()
        popen["creationflags"] = popen.get("creationflags", 0) | CREATE_SUSPENDED
        job.proc = subprocess.Popen(cmd, **popen)
        handle = wt.HANDLE(int(job.proc._handle))
        if not k32.AssignProcessToJobObject(job.handle, handle):
            raise ctypes.WinError(ctypes.get_last_error())
        ntdll.NtResumeProcess(handle)
        return job

    def cpu_seconds(self):
        info = ACCOUNTING()
        k32.QueryInformationJobObject(self.handle, JobObjectBasicAccountingInformation,
                                      ctypes.byref(info), ctypes.sizeof(info), None)
        return (info.TotalUserTime + info.TotalKernelTime) / 1e7

    def total_processes(self):
        info = ACCOUNTING()
        k32.QueryInformationJobObject(self.handle, JobObjectBasicAccountingInformation,
                                      ctypes.byref(info), ctypes.sizeof(info), None)
        return info.TotalProcesses

    def peak_commit_bytes(self):
        info = EXTENDED_LIMIT()
        k32.QueryInformationJobObject(self.handle, JobObjectExtendedLimitInformation,
                                      ctypes.byref(info), ctypes.sizeof(info), None)
        return info.PeakJobMemoryUsed

    def pids(self):
        n = 4096

        class LIST(ctypes.Structure):
            _fields_ = [("Assigned", wt.DWORD), ("InList", wt.DWORD), ("Ids", ctypes.c_size_t * n)]

        lst = LIST()
        k32.QueryInformationJobObject(self.handle, JobObjectBasicProcessIdList,
                                      ctypes.byref(lst), ctypes.sizeof(lst), None)
        return [int(lst.Ids[i]) for i in range(lst.InList)]

    def terminate(self):
        k32.TerminateJobObject(self.handle, 1)

    def close(self):
        k32.CloseHandle(self.handle)
